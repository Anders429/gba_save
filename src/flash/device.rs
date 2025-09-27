use core::{
    fmt,
    fmt::{Display, Formatter},
};
#[cfg(feature = "serde")]
use serde::{
    de::{Deserialize, Deserializer, Visitor},
    ser::{Serialize, Serializer},
};

/// An unknown device ID.
///
/// There are several different common devices used in GBA cartridges for flash data. These devices
/// identify themselves using an ID. When [`Flash`] is initialized, it attempts to identify the
/// current device. This type will be returned if the device returned an unrecognized ID.
///
/// An unknown device ID indicates that the driver cannot tell what type of device is installed,
/// and therefore cannot know how to interact with it.
///
/// [`Flash`]: crate::flash::Flash
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UnknownDeviceId(pub u16);

impl Display for UnknownDeviceId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "Unknown Device ID: 0x{:04x}", self.0)
    }
}

impl core::error::Error for UnknownDeviceId {}

#[cfg(feature = "serde")]
impl Serialize for UnknownDeviceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("UnknownDeviceId", &self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for UnknownDeviceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UnknownDeviceIdVisitor;

        impl<'de> Visitor<'de> for UnknownDeviceIdVisitor {
            type Value = UnknownDeviceId;

            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
                formatter.write_str("struct UnknownDeviceId")
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                u16::deserialize(deserializer).map(|id| UnknownDeviceId(id))
            }
        }

        deserializer.deserialize_newtype_struct("UnknownDeviceId", UnknownDeviceIdVisitor)
    }
}

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
    type Error = UnknownDeviceId;

    fn try_from(id: u16) -> Result<Self, Self::Error> {
        match id {
            0x09c2 => Ok(Device::MX29L010),
            0x1362 => Ok(Device::LE26FV10N1TS),
            0x1b32 => Ok(Device::MN63F805MNP),
            0x1cc2 => Ok(Device::MX29L512),
            0x3d1f => Ok(Device::AT29LV512),
            0xd4bf => Ok(Device::LE39FW512),
            _ => Err(UnknownDeviceId(id)),
        }
    }
}

impl Display for Device {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MX29L010 => "Macronix 128KiB (ID: 0x09c2, chip: MX29L010)",
            Self::LE26FV10N1TS => "Sanyo 128KiB (ID: 0x1362, chip: LE26FV10N1TS)",
            Self::MN63F805MNP => "Panasonic 64KiB (ID: 0x1b32, chip: MN63F805MNP)",
            Self::MX29L512 => "Macronix 64KiB (ID: 0x1cc2, chip: MX29L512)",
            Self::AT29LV512 => "Atmel 64KiB (ID: 0x3d1f, chip: AT29LV512)",
            Self::LE39FW512 => "SST 64KiB (ID: 0xd4bf, chip: LE39FW512)",
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use super::{Device, UnknownDeviceId};
    use alloc::format;
    #[cfg(feature = "serde")]
    use claims::assert_ok;
    use claims::{assert_err_eq, assert_ok_eq};
    use gba_test::test;
    #[cfg(feature = "serde")]
    use serde::{Deserialize, Serialize};
    #[cfg(feature = "serde")]
    use serde_assert::{Deserializer, Serializer, Token};

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
        assert_ok_eq!(Device::try_from(0xd4bf), Device::LE39FW512);
    }

    #[test]
    fn device_from_unknown() {
        assert_err_eq!(Device::try_from(0xffff), UnknownDeviceId(0xffff));
    }

    #[test]
    fn display_MX29L010() {
        assert_eq!(
            format!("{}", Device::MX29L010),
            "Macronix 128KiB (ID: 0x09c2, chip: MX29L010)"
        );
    }

    #[test]
    fn display_LE26FV10N1TS() {
        assert_eq!(
            format!("{}", Device::LE26FV10N1TS),
            "Sanyo 128KiB (ID: 0x1362, chip: LE26FV10N1TS)"
        );
    }

    #[test]
    fn display_MN63F805MNP() {
        assert_eq!(
            format!("{}", Device::MN63F805MNP),
            "Panasonic 64KiB (ID: 0x1b32, chip: MN63F805MNP)"
        );
    }

    #[test]
    fn display_MX29L512() {
        assert_eq!(
            format!("{}", Device::MX29L512),
            "Macronix 64KiB (ID: 0x1cc2, chip: MX29L512)"
        );
    }

    #[test]
    fn display_AT29LV512() {
        assert_eq!(
            format!("{}", Device::AT29LV512),
            "Atmel 64KiB (ID: 0x3d1f, chip: AT29LV512)"
        );
    }

    #[test]
    fn display_LE39FW512() {
        assert_eq!(
            format!("{}", Device::LE39FW512),
            "SST 64KiB (ID: 0xd4bf, chip: LE39FW512)"
        );
    }

    #[test]
    fn display_unknown() {
        assert_eq!(
            format!("{}", UnknownDeviceId(0x0123)),
            "Unknown Device ID: 0x0123"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn unknown_device_id_serialize() {
        let serializer = Serializer::builder().build();
        assert_ok_eq!(
            UnknownDeviceId(0x0123).serialize(&serializer),
            [
                Token::NewtypeStruct {
                    name: "UnknownDeviceId",
                },
                Token::U16(0x0123)
            ]
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn unknown_device_id_deserialize() {
        let mut deserializer = Deserializer::builder([
            Token::NewtypeStruct {
                name: "UnknownDeviceId",
            },
            Token::U16(0x0123),
        ])
        .build();
        assert_ok_eq!(
            UnknownDeviceId::deserialize(&mut deserializer),
            UnknownDeviceId(0x0123)
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn unknown_device_id_serde_roundtrip() {
        let serializer = Serializer::builder().build();
        let mut deserializer =
            Deserializer::builder(assert_ok!(UnknownDeviceId(0x0123).serialize(&serializer)))
                .build();
        assert_ok_eq!(
            UnknownDeviceId::deserialize(&mut deserializer),
            UnknownDeviceId(0x0123)
        );
    }
}
