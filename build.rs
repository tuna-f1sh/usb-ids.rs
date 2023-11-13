use std::env;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use phf_codegen::Map;
use phf_shared::{PhfHash, FmtConst};
use quote::quote;

/* This build script contains a "parser" for the USB ID database.
 * "Parser" is in scare-quotes because it's really a line matcher with a small amount
 * of context needed for pairing nested entities (e.g. devices) with their parents (e.g. vendors).
 */

const VENDOR_PROLOGUE: &str = "static USB_IDS: phf::Map<u16, Vendor> = ";
const CLASS_PROLOGUE: &str = "static USB_CLASSES: phf::Map<u8, Class> = ";

type VMap = Map<u16>;
type CMap = Map<u8>;

struct CgVendor {
    id: u16,
    name: String,
    devices: Vec<CgDevice>,
}

struct CgDevice {
    id: u16,
    name: String,
    interfaces: Vec<CgInterface>,
}

struct CgInterface {
    id: u8,
    name: String,
}

struct CgClass {
    id: u8,
    name: String,
    sub_classes: Vec<CgSubClass>,
}

struct CgSubClass {
    id: u8,
    name: String,
    protocols: Vec<CgProtocol>,
}

struct CgProtocol {
    id: u8,
    name: String,
}

struct CgAtType {
    id: u16,
    name: String,
}

/// Parser state expects file in be in this order. It's required because some
/// parsers are ambiguous without context; device.interface == subclass.protocol for example.
enum ParserState {
    Vendors(Option<CgVendor>, u16),
    Classes(Option<CgClass>, u8),
    Types,
}

#[allow(clippy::redundant_field_names)]
fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let src_path = Path::new("src/usb.ids");
    let dest_path = Path::new(&out_dir).join("usb_ids.cg.rs");
    let input = {
        let f = fs::File::open(src_path).unwrap();
        BufReader::new(f)
    };
    let mut output = {
        let f = fs::File::create(dest_path).unwrap();
        BufWriter::new(f)
    };

    // Parser state.
    let mut parser_state = ParserState::Vendors(None, 0u16);

    let mut vmap = VMap::new();
    let mut cmap = CMap::new();

    for line in input.lines() {
        let line = line.unwrap();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with("C ") && !matches!(parser_state, ParserState::Classes(_, _)) {
            // If there was a previous vendor, emit it here before switch
            if let ParserState::Vendors(Some(vendor), _) = parser_state {
                emit_vendor(&mut vmap, &vendor);
            }
            parser_state = ParserState::Classes(None, 0u8);
        // this relies on Audio Terminal Types being first after classes...
        } else if line.starts_with("AT ") && !matches!(parser_state, ParserState::Types)  {
            // If there was a previous class, emit it here before switch
            if let ParserState::Classes(Some(class), _) = parser_state {
                emit_class(&mut cmap, &class);
            }
            parser_state = ParserState::Types;
        }

        match parser_state {
            ParserState::Vendors(ref mut curr_vendor, ref mut curr_device_id) => {
                if let Ok((name, id)) = parser::vendor(&line) {
                    // If there was a previous vendor, emit it.
                    if let Some(vendor) = curr_vendor.take() {
                        emit_vendor(&mut vmap, &vendor);
                    }

                    // Set our new vendor as the current vendor.
                    *curr_vendor = Some(CgVendor {
                        id,
                        name: name.into(),
                        devices: vec![],
                    });
                // We should always have a current vendor; failure here indicates a malformed input.
                } else if let Some(curr_vendor) = curr_vendor.as_mut() {
                    if let Ok((name, id)) = parser::device(&line) {
                        curr_vendor.devices.push(CgDevice {
                            id,
                            name: name.into(),
                            interfaces: vec![],
                        });
                        *curr_device_id = id;
                    } else if let Ok((name, id)) = parser::interface(&line) {
                        let curr_device = curr_vendor
                            .devices
                            .iter_mut()
                            .find(|d| d.id == *curr_device_id)
                            .expect("No parent device whilst parsing interfaces, confirm file not malformed");

                        curr_device.interfaces.push(CgInterface {
                            id,
                            name: name.into(),
                        });
                    }
                } else {
                    panic!("No parent vendor whilst parsing vendors, confirm file in correct order and not malformed: {:?}", line);
                }
            }
            ParserState::Classes(ref mut curr_class, ref mut curr_class_id) => {
                if let Ok((name, id)) = parser::class(&line) {
                    // If there was a previous class, emit it.
                    if let Some(class) = curr_class.take() {
                        emit_class(&mut cmap, &class);
                    }

                    // Set our new class as the current class.
                    *curr_class = Some(CgClass {
                        id,
                        name: name.into(),
                        sub_classes: vec![],
                    });
                // We should always have a current class; failure here indicates a malformed input.
                } else if let Some(curr_class) = curr_class.as_mut() {
                    if let Ok((name, id)) = parser::sub_class(&line) {
                        curr_class.sub_classes.push(CgSubClass {
                            id,
                            name: name.into(),
                            protocols: vec![],
                        });
                        *curr_class_id = id;
                    } else if let Ok((name, id)) = parser::protocol(&line) {
                        let curr_device = curr_class
                            .sub_classes
                            .iter_mut()
                            .find(|d| d.id == *curr_class_id)
                            .expect("No parent sub-class whilst parsing protocols, confirm file not malformed");

                        curr_device.protocols.push(CgProtocol {
                            id,
                            name: name.into(),
                        });
                    }
                } else {
                    panic!("No parent class whilst parsing classes, confirm file in correct order and not malformed: {:?}", line);
                }
            },
            ParserState::Types => {
                break;
            }
        }
    }

    emit_epilogue(VENDOR_PROLOGUE, &mut output, vmap);
    emit_epilogue(CLASS_PROLOGUE, &mut output, cmap);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/usb.ids");
}

mod parser {
    use std::num::ParseIntError;

    use nom::bytes::complete::{tag, take};
    use nom::character::complete::{hex_digit1, tab};
    use nom::combinator::{all_consuming, map_parser, map_res};
    use nom::sequence::{delimited, terminated};
    use nom::IResult;

    fn id<T, F>(size: usize, from_str_radix: F) -> impl Fn(&str) -> IResult<&str, T>
    where
        F: Fn(&str, u32) -> Result<T, ParseIntError>,
    {
        move |input| {
            map_res(map_parser(take(size), all_consuming(hex_digit1)), |input| {
                from_str_radix(input, 16)
            })(input)
        }
    }

    pub fn vendor(input: &str) -> IResult<&str, u16> {
        let id = id(4, u16::from_str_radix);
        terminated(id, tag("  "))(input)
    }

    pub fn device(input: &str) -> IResult<&str, u16> {
        let id = id(4, u16::from_str_radix);
        delimited(tab, id, tag("  "))(input)
    }

    pub fn interface(input: &str) -> IResult<&str, u8> {
        let id = id(2, u8::from_str_radix);
        delimited(tag("\t\t"), id, tag("  "))(input)
    }

    pub fn class(input: &str) -> IResult<&str, u8> {
        let id = id(2, u8::from_str_radix);
        delimited(tag("C "), id, tag("  "))(input)
    }

    pub fn sub_class(input: &str) -> IResult<&str, u8> {
        let id = id(2, u8::from_str_radix);
        delimited(tab, id, tag("  "))(input)
    }

    pub fn protocol(input: &str) -> IResult<&str, u8> {
        let id = id(2, u8::from_str_radix);
        delimited(tag("\t\t"), id, tag("  "))(input)
    }
}

fn emit_vendor(map: &mut VMap, vendor: &CgVendor) {
    map.entry(vendor.id, &quote!(#vendor).to_string());
}

fn emit_class(map: &mut CMap, class: &CgClass) {
    map.entry(class.id, &quote!(#class).to_string());
}

fn emit_epilogue<K: Eq + std::hash::Hash + PhfHash + FmtConst>(prologue_str: &'static str, output: &mut impl Write, map: Map<K>) {
    writeln!(output, "{}", prologue_str).unwrap();
    writeln!(output, "{};", map.build()).unwrap();
}

impl quote::ToTokens for CgVendor {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let CgVendor {
            id: vendor_id,
            name,
            devices,
        } = self;

        let devices = devices.iter().map(|CgDevice { id, name, interfaces }| {
            quote!{
                Device { vendor_id: #vendor_id, id: #id, name: #name, interfaces: &[#(#interfaces),*] }
            }
        });
        tokens.extend(quote! {
            Vendor { id: #vendor_id, name: #name, devices: &[#(#devices),*] }
        });
    }
}

impl quote::ToTokens for CgInterface {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let CgInterface { id, name } = self;
        tokens.extend(quote! {
            Interface { id: #id, name: #name }
        });
    }
}

impl quote::ToTokens for CgClass {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let CgClass {
            id: class_id,
            name,
            sub_classes,
        } = self;

        let sub_classes = sub_classes.iter().map(|CgSubClass { id, name, protocols }| {
            quote!{
                SubClass { class_id: #class_id, id: #id, name: #name, protocols: &[#(#protocols),*] }
            }
        });
        tokens.extend(quote! {
            Class { id: #class_id, name: #name, sub_classes: &[#(#sub_classes),*] }
        });
    }
}

impl quote::ToTokens for CgProtocol {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let CgProtocol { id, name } = self;
        tokens.extend(quote! {
            Protocol { id: #id, name: #name }
        });
    }
}
