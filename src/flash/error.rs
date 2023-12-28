use embedded_io::ErrorKind;

/// An error that can occur when writing to flash memory.
#[derive(Debug)]
pub enum Error {
    /// The write operation did not complete successfully within the device's timeout window.
    OperationTimedOut,

    /// The writer has exhausted all of its space.
    ///
    /// This indicates that the range provided when creating the writer has been completely
    /// exhausted.
    EndOfWriter,
}

impl embedded_io::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::OperationTimedOut => ErrorKind::TimedOut,
            Self::EndOfWriter => ErrorKind::WriteZero,
        }
    }
}
