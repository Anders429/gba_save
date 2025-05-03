macro_rules! info {
    ($($tokens:tt)*) => {
        #[cfg(feature = "log")]
        {
            ::log::info!($($tokens)*)
        }
    }
}

// We rename this macro at export to avoid a conflict with a builtin attribute also named `warn`.
macro_rules! _warn {
    ($($tokens:tt)*) => {
        #[cfg(feature = "log")]
        {
            ::log::warn!($($tokens)*)
        }
    }
}

#[cfg(feature = "log")]
pub(crate) use _warn as warn;
pub(crate) use info;
