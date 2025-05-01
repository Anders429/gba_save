use core::{
    fmt,
    fmt::{Display, Formatter},
};
use embedded_io::ErrorKind;

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
