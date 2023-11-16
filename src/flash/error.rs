use embedded_io::ErrorKind;

#[derive(Debug)]
pub enum Error {
    OperationTimedOut,
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
