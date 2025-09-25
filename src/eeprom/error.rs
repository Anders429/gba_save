use core::{
    fmt,
    fmt::{Display, Formatter},
};
use embedded_io::ErrorKind;
#[cfg(feature = "serde")]
use serde::{
    de,
    de::{Deserialize, Deserializer, EnumAccess, Unexpected, VariantAccess, Visitor},
    ser::{Serialize, Serializer},
};

/// An error that can occur when writing to EEPROM memory.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    /// The write operation did not complete successfully within the device's timeout window.
    ///
    /// For EEPROM, this means the device did not return a "Ready" status.
    OperationTimedOut,

    /// Data written was unable to be verified.
    WriteFailure,

    /// The writer has exhausted all of its space.
    ///
    /// This indicates that the range provided when creating the writer has been completely
    /// exhausted.
    EndOfWriter,
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OperationTimedOut => "the operation on the EEPROM device timed out",
            Self::WriteFailure => "unable to verify that data was written correctly",
            Self::EndOfWriter => "the writer has reached the end of its range",
        })
    }
}

impl core::error::Error for Error {}

impl embedded_io::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::OperationTimedOut => ErrorKind::TimedOut,
            Self::WriteFailure => ErrorKind::NotConnected,
            Self::EndOfWriter => ErrorKind::WriteZero,
        }
    }
}

impl From<embedded_io::ReadExactError<Error>> for Error {
    fn from(read_exact_error: embedded_io::ReadExactError<Error>) -> Self {
        match read_exact_error {
            embedded_io::ReadExactError::UnexpectedEof => Self::EndOfWriter,
            embedded_io::ReadExactError::Other(error) => error,
        }
    }
}

#[cfg(feature = "serde")]
impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::OperationTimedOut => {
                serializer.serialize_unit_variant("Error", 0, "OperationTimedOut")
            }
            Self::WriteFailure => serializer.serialize_unit_variant("Error", 1, "WriteFailure"),
            Self::EndOfWriter => serializer.serialize_unit_variant("Error", 2, "EndOfWriter"),
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Error {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Variant {
            OperationTimedOut,
            WriteFailure,
            EndOfWriter,
        }

        impl<'de> Deserialize<'de> for Variant {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct VariantVisitor;

                impl<'de> Visitor<'de> for VariantVisitor {
                    type Value = Variant;

                    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
                        formatter.write_str("`OperationTimedOut` or `EndOfWriter`")
                    }

                    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            0 => Ok(Variant::OperationTimedOut),
                            1 => Ok(Variant::WriteFailure),
                            2 => Ok(Variant::EndOfWriter),
                            _ => Err(E::invalid_value(Unexpected::Unsigned(value), &self)),
                        }
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "OperationTimedOut" => Ok(Variant::OperationTimedOut),
                            "WriteFailure" => Ok(Variant::WriteFailure),
                            "EndOfWriter" => Ok(Variant::EndOfWriter),
                            _ => Err(E::unknown_variant(value, VARIANTS)),
                        }
                    }

                    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            b"OperationTimedOut" => Ok(Variant::OperationTimedOut),
                            b"WriteFailure" => Ok(Variant::WriteFailure),
                            b"EndOfWriter" => Ok(Variant::EndOfWriter),
                            _ => match str::from_utf8(value) {
                                Ok(value) => Err(E::unknown_variant(value, VARIANTS)),
                                Err(_) => Err(E::invalid_value(Unexpected::Bytes(value), &self)),
                            },
                        }
                    }
                }

                deserializer.deserialize_identifier(VariantVisitor)
            }
        }

        struct ErrorVisitor;

        impl<'de> Visitor<'de> for ErrorVisitor {
            type Value = Error;

            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
                formatter.write_str("enum Error")
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: EnumAccess<'de>,
            {
                match data.variant()? {
                    (Variant::OperationTimedOut, variant) => {
                        variant.unit_variant().map(|()| Error::OperationTimedOut)
                    }
                    (Variant::WriteFailure, variant) => {
                        variant.unit_variant().map(|()| Error::WriteFailure)
                    }
                    (Variant::EndOfWriter, variant) => {
                        variant.unit_variant().map(|()| Error::EndOfWriter)
                    }
                }
            }
        }

        const VARIANTS: &[&str] = &["OperationTimedOut", "WriteFailure", "EndOfWriter"];
        deserializer.deserialize_enum("Error", VARIANTS, ErrorVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use alloc::format;
    #[cfg(feature = "serde")]
    use claims::{assert_ok, assert_ok_eq};
    use gba_test::test;
    #[cfg(feature = "serde")]
    use serde::{Deserialize, Serialize};
    #[cfg(feature = "serde")]
    use serde_assert::{Deserializer, Serializer, Token};

    #[test]
    fn operation_timed_out_display() {
        assert_eq!(
            format!("{}", Error::OperationTimedOut),
            "the operation on the EEPROM device timed out"
        );
    }

    #[test]
    fn write_failure_display() {
        assert_eq!(
            format!("{}", Error::WriteFailure),
            "unable to verify that data was written correctly"
        );
    }

    #[test]
    fn end_of_writer_display() {
        assert_eq!(
            format!("{}", Error::EndOfWriter),
            "the writer has reached the end of its range"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn operation_timed_out_serialize() {
        let serializer = Serializer::builder().build();
        assert_ok_eq!(
            Error::OperationTimedOut.serialize(&serializer),
            [Token::UnitVariant {
                name: "Error",
                variant_index: 0,
                variant: "OperationTimedOut",
            }]
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn operation_timed_out_deserialize() {
        let mut deserializer = Deserializer::builder([Token::UnitVariant {
            name: "Error",
            variant_index: 0,
            variant: "OperationTimedOut",
        }])
        .build();
        assert_ok_eq!(
            Error::deserialize(&mut deserializer),
            Error::OperationTimedOut
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn operation_timed_out_serde_roundtrip() {
        let serializer = Serializer::builder().build();
        let mut deserializer =
            Deserializer::builder(assert_ok!(Error::OperationTimedOut.serialize(&serializer)))
                .build();
        assert_ok_eq!(
            Error::deserialize(&mut deserializer),
            Error::OperationTimedOut
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn write_failure_serialize() {
        let serializer = Serializer::builder().build();
        assert_ok_eq!(
            Error::WriteFailure.serialize(&serializer),
            [Token::UnitVariant {
                name: "Error",
                variant_index: 1,
                variant: "WriteFailure",
            }]
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn write_failure_deserialize() {
        let mut deserializer = Deserializer::builder([Token::UnitVariant {
            name: "Error",
            variant_index: 1,
            variant: "WriteFailure",
        }])
        .build();
        assert_ok_eq!(Error::deserialize(&mut deserializer), Error::WriteFailure);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn write_failure_serde_roundtrip() {
        let serializer = Serializer::builder().build();
        let mut deserializer =
            Deserializer::builder(assert_ok!(Error::WriteFailure.serialize(&serializer))).build();
        assert_ok_eq!(Error::deserialize(&mut deserializer), Error::WriteFailure);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn end_of_writer_serialize() {
        let serializer = Serializer::builder().build();
        assert_ok_eq!(
            Error::EndOfWriter.serialize(&serializer),
            [Token::UnitVariant {
                name: "Error",
                variant_index: 2,
                variant: "EndOfWriter",
            }]
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn end_of_writer_deserialize() {
        let mut deserializer = Deserializer::builder([Token::UnitVariant {
            name: "Error",
            variant_index: 2,
            variant: "EndOfWriter",
        }])
        .build();
        assert_ok_eq!(Error::deserialize(&mut deserializer), Error::EndOfWriter);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn end_of_writer_serde_roundtrip() {
        let serializer = Serializer::builder().build();
        let mut deserializer =
            Deserializer::builder(assert_ok!(Error::EndOfWriter.serialize(&serializer))).build();
        assert_ok_eq!(Error::deserialize(&mut deserializer), Error::EndOfWriter);
    }

    #[test]
    fn read_exact_error_end_of_file_into_error() {
        assert_eq!(
            Error::from(embedded_io::ReadExactError::UnexpectedEof),
            Error::EndOfWriter
        );
    }

    #[test]
    fn read_exact_error_other_into_error() {
        assert_eq!(
            Error::from(embedded_io::ReadExactError::Other(Error::OperationTimedOut)),
            Error::OperationTimedOut
        );
    }
}
