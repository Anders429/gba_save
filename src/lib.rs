//! Tools for interacting with backup media on Game Boy Advance cartridges.
//!
//! The Game Boy Advance has three forms of backup media for saving data: SRAM, EEPROM, and Flash.
//! This library provides tools for interacting with all three different types, in the various
//! sizes that are available for each one.
//!
//! # Range-Based Interfaces
//! When reading from and writing to backup media using this library, you will need to specify the
//! range over which you wish to read and write. These ranges are defined using types from the
//! [`deranged`] crate (usually [`RangedUsize`]). This provides stronger type safety: rather than
//! the logic provided by this crate having to check for range validity on each usage, it is
//! instead verified using the type system.
//!
//! # Input and Output Traits
//! The various readers and writers provided by this library implement the [`Read`] and [`Write`]
//! traits from the [`embedded_io`] crate. The types implementing these traits will "remember" how
//! far into their defined range they have read or written, allowing you to interact with a large
//! range easily.
//!
//! # Example
//! To write and read save data using SRAM, use something like the following:
//! ```
//! use deranged::RangedUsize;
//! use embedded_io::{Read, Write};
//! use gba_save::sram::Sram;
//!
//! let mut sram = unsafe {Sram::new()};
//! let mut writer = sram.writer(RangedUsize::new_static<0>()..RangedUsize::new_static<15>());
//!
//! // Write some data.
//! //
//! // Note that you'll usually want to handle the error here.
//! writer.write(b"hello, world!").expect("could not write to SRAM");
//!
//! // Write some more data.
//! writer.write(b"123").expect("could not write to SRAM");
//!
//! // Read the data back.
//! let mut reader = sram.reader(RangedUsize::new_static<0>()..RangedUsize::new_static<15>());
//! let mut buffer = [0; 16];
//! assert_eq!(reader.read(&mut buf), 16);
//! // Both things that were written will be read back.
//! assert_eq!(buffer, b"hello, world!123");
//! ```
//!
//! # Optional Features
//! - **`serde`**: Enable serializing and deserializing the variuos error types using the
//! [`serde`](https://docs.rs/serde/latest/serde/) library.
//! - **`log`**: Enable log messages using the [`log`](https://docs.rs/log/latest/log/) library.
//! Helpful for development. This is best used when paired with a logger like [`mgba_log`] or
//! [`nocash_gba_log`](https://docs.rs/nocash_gba_log/latest/nocash_gba_log/).
//!
//! [`RangedUsize`]: deranged::RangedUsize
//! [`Read`]: embedded_io::Read
//! [`Write`]: embedded_io::Write

#![no_std]
#![cfg_attr(test, no_main)]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(gba_test::runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_harness")]

#[cfg(test)]
extern crate alloc;

pub mod eeprom;
pub mod flash;
pub mod sram;

mod log;
mod mmio;
mod range;

#[cfg(test)]
#[no_mangle]
pub fn main() {
    let _ = mgba_log::init();
    test_harness()
}
