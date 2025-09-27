//! Wrappers around log macros.
//!
//! These allow logging within the crate without having to sprinkle `#[cfg(feature = "log")]` all
//! over the place.

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

pub(crate) use _warn as warn;
pub(crate) use info;
