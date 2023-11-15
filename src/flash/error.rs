use embedded_io::ErrorKind;

#[derive(Debug)]
pub enum Error {
    UnknownDeviceID(u16),
    OperationTimedOut,
    EndOfWriter,
}

impl embedded_io::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::UnknownDeviceID(_) => ErrorKind::Unsupported,
            Self::OperationTimedOut => ErrorKind::TimedOut,
            Self::EndOfWriter => ErrorKind::WriteZero,
        }
    }
}
