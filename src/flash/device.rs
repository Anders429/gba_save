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
#[derive(Debug)]
pub struct UnknownDeviceID(pub(crate) u16);

/// Different flash chip devices, by ID code.
#[derive(Clone, Copy, Debug)]
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
