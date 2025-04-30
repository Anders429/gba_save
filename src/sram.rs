//! SRAM backup memory.
//!
//! Unlike other backup memory types, SRAM only comes in a single size: 32KiB. This makes it
//! simpler to work with than flash or EEPROM, as you don't have to determine what size you are
//! supporting.
//!
//! To interact with SRAM, use the [`Sram`] type to create readers and writers over ranges of SRAM
//! memory.

use crate::{
    mmio::{Cycles, WAITCNT},
    range::translate_range_to_buffer,
};
use core::{
    cmp::min,
    convert::Infallible,
    fmt,
    fmt::{Display, Formatter},
    marker::PhantomData,
    ops::RangeBounds,
};
use deranged::RangedUsize;
use embedded_io::{ErrorKind, ErrorType, Read, Write};

const SRAM_MEMORY: *mut u8 = 0x0e00_0000 as *mut u8;

/// A reader on SRAM.
///
/// This type allows reading data over the range specified upon creation.
pub struct Reader<'a> {
    address: *mut u8,
    len: usize,
    lifetime: PhantomData<&'a ()>,
}

impl Reader<'_> {
    unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        Self {
            address,
            len,
            lifetime: PhantomData,
        }
    }
}

impl ErrorType for Reader<'_> {
    type Error = Infallible;
}

impl Read for Reader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let mut read_count = 0;
        loop {
            if read_count >= min(buf.len(), self.len) {
                self.address = unsafe { self.address.add(read_count) };
                self.len -= read_count;
                return Ok(read_count);
            }

            unsafe {
                *buf.get_unchecked_mut(read_count) = self.address.add(read_count).read_volatile();
            }
            read_count += 1;
        }
    }
}

/// An error that can occur when writing to SRAM memory.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
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
            Self::WriteFailure => "unable to verify that data was written correctly",
            Self::EndOfWriter => "the writer has reached the end of its range",
        })
    }
}

impl core::error::Error for Error {}

impl embedded_io::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::WriteFailure => ErrorKind::NotConnected,
            Self::EndOfWriter => ErrorKind::WriteZero,
        }
    }
}

fn verify_byte(address: *const u8, byte: u8) -> Result<(), Error> {
    if unsafe { address.read_volatile() } == byte {
        Ok(())
    } else {
        Err(Error::WriteFailure)
    }
}

/// A writer on SRAM.
///
/// This type allows writing data on the range specified upon creation.
pub struct Writer<'a> {
    address: *mut u8,
    len: usize,
    lifetime: PhantomData<&'a ()>,
}

impl Writer<'_> {
    unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        Self {
            address,
            len,
            lifetime: PhantomData,
        }
    }
}

impl ErrorType for Writer<'_> {
    type Error = Error;
}

impl Write for Writer<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let mut write_count = 0;
        loop {
            if write_count >= min(buf.len(), self.len) {
                if self.len == 0 {
                    return Err(Error::EndOfWriter);
                }
                self.address = unsafe { self.address.add(write_count) };
                self.len -= write_count;
                return Ok(write_count);
            }

            let address = unsafe { self.address.add(write_count) };
            let byte = unsafe { *buf.get_unchecked(write_count) };
            unsafe {
                address.write_volatile(byte);
            }
            verify_byte(address, byte)?;

            write_count += 1;
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Access to SRAM backup.
pub struct Sram {
    /// As this struct maintains ownership of SRAM memory and WAITCNT's SRAM wait control setting,
    /// we want to make sure it can only be constructed through its `unsafe` `new()` associated
    /// function.
    _private: (),
}

impl Sram {
    /// Creates an accessor to the SRAM backup.
    ///
    /// # Safety
    /// Must have exclusive ownership of both SRAM memory and WAITCNT’s SRAM wait control setting
    /// for the duration of its lifetime.
    pub unsafe fn new() -> Self {
        let mut waitstate_control = unsafe { WAITCNT.read_volatile() };
        waitstate_control.set_backup_waitstate(Cycles::_8);
        unsafe { WAITCNT.write_volatile(waitstate_control) };

        Self { _private: () }
    }

    /// Returns a reader over the given range.
    pub fn reader<'a, 'b, Range>(&'a self, range: Range) -> Reader<'b>
    where
        Range: RangeBounds<RangedUsize<0, 32767>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range, SRAM_MEMORY);
        unsafe { Reader::new_unchecked(address, len) }
    }

    /// Returns a writer over the given range.
    pub fn writer<'a, 'b, Range>(&'a mut self, range: Range) -> Writer<'b>
    where
        Range: RangeBounds<RangedUsize<0, 32767>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range, SRAM_MEMORY);
        unsafe { Writer::new_unchecked(address, len) }
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Sram};
    use claims::{assert_err_eq, assert_ok_eq};
    use deranged::RangedUsize;
    use embedded_io::{Read, Write};
    use gba_test::test;

    #[test]
    #[cfg_attr(
        not(sram),
        ignore = "This test requires an SRAM chip. Ensure SRAM is configured and pass `--cfg sram` to enable."
    )]
    fn empty_range_read() {
        let sram = unsafe { Sram::new() };
        let mut reader =
            sram.reader(RangedUsize::new_static::<0>()..RangedUsize::new_static::<0>());

        let mut buf = [1, 2, 3, 4];
        assert_ok_eq!(reader.read(&mut buf), 0);
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    #[test]
    #[cfg_attr(
        not(sram),
        ignore = "This test requires an SRAM chip. Ensure SRAM is configured and pass `--cfg sram` to enable."
    )]
    fn empty_range_write() {
        let mut sram = unsafe { Sram::new() };
        let mut writer =
            sram.writer(RangedUsize::new_static::<0>()..RangedUsize::new_static::<0>());

        assert_err_eq!(writer.write(&[0]), Error::EndOfWriter);
    }

    #[test]
    #[cfg_attr(
        not(sram),
        ignore = "This test requires an SRAM chip. Ensure SRAM is configured and pass `--cfg sram` to enable."
    )]
    fn full_range() {
        let mut sram = unsafe { Sram::new() };
        let mut writer = sram.writer(..);

        for i in 0..8192 {
            assert_ok_eq!(
                writer.write(&[
                    0u8.wrapping_add(i as u8),
                    1u8.wrapping_add(i as u8),
                    2u8.wrapping_add(i as u8),
                    3u8.wrapping_add(i as u8)
                ]),
                4
            );
        }

        let mut reader = sram.reader(..);
        let mut buf = [0, 0, 0, 0];

        for i in 0..8192 {
            assert_ok_eq!(reader.read(&mut buf), 4);
            assert_eq!(
                buf,
                [
                    0u8.wrapping_add(i as u8),
                    1u8.wrapping_add(i as u8),
                    2u8.wrapping_add(i as u8),
                    3u8.wrapping_add(i as u8)
                ]
            );
        }
    }

    #[test]
    #[cfg_attr(
        not(sram),
        ignore = "This test requires an SRAM chip. Ensure SRAM is configured and pass `--cfg sram` to enable."
    )]
    fn partial_range() {
        let mut sram = unsafe { Sram::new() };
        let mut writer =
            sram.writer(RangedUsize::new_static::<42>()..RangedUsize::new_static::<100>());

        assert_ok_eq!(writer.write(&[b'a'; 100]), 58);

        let mut reader =
            sram.reader(RangedUsize::new_static::<51>()..RangedUsize::new_static::<60>());
        let mut buf = [0; 20];

        assert_ok_eq!(reader.read(&mut buf), 9);
        assert_eq!(
            buf,
            [
                b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a', b'a', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0
            ]
        );
    }

    #[test]
    #[cfg_attr(
        sram,
        ignore = "This test cannot be run with an SRAM chip. Ensure SRAM is not configured and do not pass `--cfg sram` to enable."
    )]
    fn write_failure() {
        let mut sram = unsafe { Sram::new() };
        let mut writer = sram.writer(..);

        assert_err_eq!(writer.write(b"hello, world!"), Error::WriteFailure);
    }
}
