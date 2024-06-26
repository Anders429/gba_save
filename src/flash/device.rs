/// An unknown device ID.
///
/// There are several different common devices used in GBA cartridges for flash data. These devices
/// identify themselves using an ID. When [`Flash`] is initialized, it attempts to identify the
/// current device. This type will be returned if the device returned an unrecognized ID.
///
/// An unknown device ID indicates that the driver cannot tell what type of device is installed,
/// and therefore cannot know how to interact with it.
///
/// [`Flash`]: gba_save::flash::Flash
#[derive(Debug, Eq, PartialEq)]
pub struct UnknownDeviceID(pub u16);

/// Different flash chip devices, by ID code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Device {
    /// Macronix 128K
    MX29L010,
    /// Sanyo
    LE26FV10N1TS,
    /// Panasonic
    MN63F805MNP,
    /// Macronix 64K
    MX29L512,
    /// Atmel
    AT29LV512,
    /// SST
    LE39FW512,
}

impl TryFrom<u16> for Device {
    type Error = UnknownDeviceID;

    fn try_from(id: u16) -> Result<Self, Self::Error> {
        match id {
            0x09c2 => Ok(Device::MX29L010),
            0x1362 => Ok(Device::LE26FV10N1TS),
            0x1b32 => Ok(Device::MN63F805MNP),
            0x1cc2 => Ok(Device::MX29L512),
            0x3d1f => Ok(Device::AT29LV512),
            0xd4b4 => Ok(Device::LE39FW512),
            _ => Err(UnknownDeviceID(id)),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use super::{Device, UnknownDeviceID};
    use claims::{assert_err_eq, assert_ok_eq};
    use gba_test::test;

    #[test]
    fn device_from_MX29L010() {
        assert_ok_eq!(Device::try_from(0x09c2), Device::MX29L010);
    }

    #[test]
    fn device_from_LE26FV10N1TS() {
        assert_ok_eq!(Device::try_from(0x1362), Device::LE26FV10N1TS);
    }

    #[test]
    fn device_from_MN63F805MNP() {
        assert_ok_eq!(Device::try_from(0x1b32), Device::MN63F805MNP);
    }

    #[test]
    fn device_from_MX29L512() {
        assert_ok_eq!(Device::try_from(0x1cc2), Device::MX29L512);
    }

    #[test]
    fn device_from_AT29LV512() {
        assert_ok_eq!(Device::try_from(0x3d1f), Device::AT29LV512);
    }

    #[test]
    fn device_from_LE39FW512() {
        assert_ok_eq!(Device::try_from(0xd4b4), Device::LE39FW512);
    }

    #[test]
    fn device_from_unknown() {
        assert_err_eq!(Device::try_from(0xffff), UnknownDeviceID(0xffff));
    }
}
