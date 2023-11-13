//!
//! Rust wrappers for the [USB ID Repository](http://www.linux-usb.org/usb-ids.html).
//!
//! The USB ID Repository is the canonical source of USB device information for most
//! Linux userspaces; this crate vendors the USB ID database to allow non-Linux hosts to
//! access the same canonical information.
//!
//! # Usage
//!
//! Iterating over all known vendors:
//!
//! ```rust
//! use usb_ids::Vendors;
//!
//! for vendor in Vendors::iter() {
//!     for device in vendor.devices() {
//!         println!("vendor: {}, device: {}", vendor.name(), device.name());
//!     }
//! }
//! ```
//!
//! See the individual documentation for each structure for more details.
//!

#![warn(missing_docs)]

// Codegen: introduces USB_IDS, a phf::Map<u16, Vendor>, USB_CLASSES, a phf::Map<u8, Class>
include!(concat!(env!("OUT_DIR"), "/usb_ids.cg.rs"));

/// An abstraction for iterating over all vendors in the USB database.
pub struct Vendors;
impl Vendors {
    /// Returns an iterator over all vendors in the USB database.
    pub fn iter() -> impl Iterator<Item = &'static Vendor> {
        USB_IDS.values()
    }
}

/// An abstraction for iterating over all classes in the USB database.
pub struct Classes;
impl Classes {
    /// Returns an iterator over all classes in the USB database.
    pub fn iter() -> impl Iterator<Item = &'static Class> {
        USB_CLASSES.values()
    }
}

/// Represents a USB device vendor in the USB database.
///
/// Every device vendor has a vendor ID, a pretty name, and a
/// list of associated [`Device`]s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vendor {
    id: u16,
    name: &'static str,
    devices: &'static [Device],
}

impl Vendor {
    /// Returns the vendor's ID.
    pub fn id(&self) -> u16 {
        self.id
    }

    /// Returns the vendor's name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Returns an iterator over the vendor's devices.
    pub fn devices(&self) -> impl Iterator<Item = &'static Device> {
        self.devices.iter()
    }
}

/// Represents a single device in the USB database.
///
/// Every device has a corresponding vendor, a device ID, a pretty name,
/// and a list of associated [`Interface`]s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Device {
    vendor_id: u16,
    id: u16,
    name: &'static str,
    interfaces: &'static [Interface],
}

impl Device {
    /// Returns the [`Device`] corresponding to the given vendor and product IDs,
    /// or `None` if no such device exists in the DB.
    pub fn from_vid_pid(vid: u16, pid: u16) -> Option<&'static Device> {
        let vendor = Vendor::from_id(vid);

        vendor.and_then(|v| v.devices().find(|d| d.id == pid))
    }

    /// Returns the [`Vendor`] that this device belongs to.
    ///
    /// Looking up a vendor by device is cheap (`O(1)`).
    pub fn vendor(&self) -> &'static Vendor {
        USB_IDS.get(&self.vendor_id).unwrap()
    }

    /// Returns a tuple of (vendor id, device/"product" id) for this device.
    ///
    /// This is convenient for interactions with other USB libraries.
    pub fn as_vid_pid(&self) -> (u16, u16) {
        (self.vendor_id, self.id)
    }

    /// Returns the device's ID.
    pub fn id(&self) -> u16 {
        self.id
    }

    /// Returns the device's name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Returns an iterator over the device's interfaces.
    ///
    /// **NOTE**: The USB database does not include interface information for
    /// most devices. This list is not authoritative.
    pub fn interfaces(&self) -> impl Iterator<Item = &'static Interface> {
        self.interfaces.iter()
    }
}

/// Represents an interface to a USB device in the USB database.
///
/// Every interface has an interface ID (which is an index on the device)
/// and a pretty name.
///
/// **NOTE**: The USB database is not a canonical or authoritative source
/// of interface information for devices. Users who wish to discover interfaces
/// on their USB devices should query those devices directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interface {
    id: u8,
    name: &'static str,
}

impl Interface {
    /// Returns the interface's ID.
    pub fn id(&self) -> u8 {
        self.id
    }

    /// Returns the interface's name.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

/// A convenience trait for retrieving a top-level entity (like a [`Vendor`]) from the USB
/// database by its unique ID.
// NOTE(ww): This trait will be generally useful once we support other top-level
// entities in `usb.ids` (like language, country code, HID codes, etc).
pub trait FromId<T> {
    /// Returns the entity corresponding to `id`, or `None` if none exists.
    fn from_id(id: T) -> Option<&'static Self>;
}

impl FromId<u16> for Vendor {
    fn from_id(id: u16) -> Option<&'static Self> {
        USB_IDS.get(&id)
    }
}

impl FromId<u8> for Class {
    fn from_id(id: u8) -> Option<&'static Self> {
        USB_CLASSES.get(&id)
    }
}

/// Represents a USB device class in the USB database.
///
/// Every device class has a class ID, a pretty name, and a
/// list of associated [`SubClass`]s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Class {
    id: u8,
    name: &'static str,
    sub_classes: &'static [SubClass],
}

impl Class {
    /// Returns the class's ID.
    pub fn id(&self) -> u8 {
        self.id
    }

    /// Returns the class's name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Returns an iterator over the class's subclasses.
    pub fn sub_classes(&self) -> impl Iterator<Item = &'static SubClass> {
        self.sub_classes.iter()
    }
}

/// Represents a class subclass in the USB database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubClass {
    class_id: u8,
    id: u8,
    name: &'static str,
    protocols: &'static [Protocol],
}

impl SubClass {
    /// Returns the [`SubClass`] corresponding to the given class and subclass IDs,
    /// or `None` if no such subclass exists in the DB.
    pub fn from_cid_scid(class_id: u8, id: u8) -> Option<&'static Self> {
        let class = Class::from_id(class_id);

        class.and_then(|c| c.sub_classes().find(|s| s.id == id))
    }

    /// Returns the [`Class`] that this subclass belongs to.
    ///
    /// Looking up a class by subclass is cheap (`O(1)`).
    pub fn class(&self) -> &'static Class {
        USB_CLASSES.get(&self.class_id).unwrap()
    }

    /// Returns a tuple of (class id, subclass id) for this subclass.
    ///
    /// This is convenient for interactions with other USB libraries.
    pub fn as_cid_scid(&self) -> (u8, u8) {
        (self.class_id, self.id)
    }

    /// Returns the subclass' ID.
    pub fn id(&self) -> u8 {
        self.id
    }

    /// Returns the subclass' name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Returns an iterator over the subclasses's protocols.
    ///
    /// **NOTE**: The USB database nor USB-IF includes protocol information for
    /// all subclassess. This list is not authoritative.
    pub fn protocols(&self) -> impl Iterator<Item = &'static Protocol> {
        self.protocols.iter()
    }
}

/// Represents a subclass protocol in the USB database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Protocol {
    id: u8,
    name: &'static str,
}

impl Protocol {
    /// Returns the [`Protocol`] corresponding to the given class, subclass, and protocol IDs,
    /// or `None` if no such protocol exists in the DB.
    pub fn from_cid_scid_pid(class_id: u8, subclass_id: u8, id: u8) -> Option<&'static Self> {
        let subclass = SubClass::from_cid_scid(class_id, subclass_id);

        subclass.and_then(|s| s.protocols().find(|p| p.id == id))
    }

    /// Returns the protocol's ID.
    pub fn id(&self) -> u8 {
        self.id
    }

    /// Returns the protocol's name.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_id() {
        let vendor = Vendor::from_id(0x1d6b).unwrap();

        assert_eq!(vendor.name(), "Linux Foundation");
        assert_eq!(vendor.id(), 0x1d6b);
    }

    #[test]
    fn test_vendor_devices() {
        let vendor = Vendor::from_id(0x1d6b).unwrap();

        for device in vendor.devices() {
            assert_eq!(device.vendor(), vendor);
            assert!(!device.name().is_empty());
        }
    }

    #[test]
    fn test_from_vid_pid() {
        let device = Device::from_vid_pid(0x1d6b, 0x0003).unwrap();

        assert_eq!(device.name(), "3.0 root hub");

        let (vid, pid) = device.as_vid_pid();

        assert_eq!(vid, device.vendor().id());
        assert_eq!(pid, device.id());

        let device2 = Device::from_vid_pid(vid, pid).unwrap();

        assert_eq!(device, device2);

        let last_device = Device::from_vid_pid(0xffee, 0x0100).unwrap();
        assert_eq!(last_device.name(), "Card Reader Controller RTS5101/RTS5111/RTS5116");
    }

    #[test]
    fn test_class_from_id() {
        let class = Class::from_id(0x03).unwrap();

        assert_eq!(class.name(), "Human Interface Device");
        assert_eq!(class.id(), 0x03);
    }

    #[test]
    fn test_subclass_from_cid_scid() {
        let subclass = SubClass::from_cid_scid(0x03, 0x01).unwrap();

        assert_eq!(subclass.name(), "Boot Interface Subclass");
        assert_eq!(subclass.id(), 0x01);
    }

    #[test]
    fn test_protocol_from_cid_scid_pid() {
        let protocol = Protocol::from_cid_scid_pid(0x03, 0x01, 0x01).unwrap();

        assert_eq!(protocol.name(), "Keyboard");
        assert_eq!(protocol.id(), 0x01);

        let protocol = Protocol::from_cid_scid_pid(0x07, 0x01, 0x03).unwrap();

        assert_eq!(protocol.name(), "IEEE 1284.4 compatible bidirectional");
        assert_eq!(protocol.id(), 0x03);

        let protocol = Protocol::from_cid_scid_pid(0xff, 0xff, 0xff).unwrap();

        // check last entry for parsing
        assert_eq!(protocol.name(), "Vendor Specific Protocol");
        assert_eq!(protocol.id(), 0xff);
    }
}
