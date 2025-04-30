use core::{
    fmt,
    fmt::{Display, Formatter},
};
use embedded_io::ErrorKind;

/// An error that can occur when writing to flash memory.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    /// The write operation did not complete successfully within the device's timeout window.
    OperationTimedOut,

    /// The writer has exhausted all of its space.
    ///
    /// This indicates that the range provided when creating the writer has been completely
    /// exhausted.
    EndOfWriter,
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OperationTimedOut => "the operation on the flash device timed out",
            Self::EndOfWriter => "the writer has reached the end of its range",
        })
    }
}

impl core::error::Error for Error {}

impl embedded_io::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::OperationTimedOut => ErrorKind::TimedOut,
            Self::EndOfWriter => ErrorKind::WriteZero,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use alloc::format;
    use embedded_io::{Error as _, ErrorKind};
    use gba_test::test;

    #[test]
    fn operation_timed_out_display() {
        assert_eq!(
            format!("{}", Error::OperationTimedOut),
            "the operation on the flash device timed out"
        );
    }

    #[test]
    fn end_of_writer_display() {
        assert_eq!(
            format!("{}", Error::EndOfWriter),
            "the writer has reached the end of its range"
        );
    }

    #[test]
    fn operation_timed_out_kind() {
        assert_eq!(Error::OperationTimedOut.kind(), ErrorKind::TimedOut);
    }

    #[test]
    fn end_of_writer_kind() {
        assert_eq!(Error::EndOfWriter.kind(), ErrorKind::WriteZero);
    }
}
