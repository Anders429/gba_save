use core::{
    cmp::min,
    convert::Infallible,
    marker::PhantomData,
    ops::{Bound, RangeBounds},
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

/// An error that can occur when writing to flash memory.
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

fn translate_range_to_buffer<const MAX: usize, Range>(range: Range) -> (*mut u8, usize)
where
    Range: RangeBounds<RangedUsize<0, MAX>>,
{
    let offset = match range.start_bound() {
        Bound::Included(start) => start.get(),
        Bound::Excluded(start) => start.get() + 1,
        Bound::Unbounded => 0,
    };
    let address = unsafe { SRAM_MEMORY.add(offset) };
    let len = match range.end_bound() {
        Bound::Included(end) => end.get() + 1,
        Bound::Excluded(end) => end.get(),
        Bound::Unbounded => MAX + 1,
    } - offset;
    (address, len)
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
        Self { _private: () }
    }

    /// Returns a reader over the given range.
    pub fn reader<'a, 'b, Range>(&'a self, range: Range) -> Reader<'b>
    where
        Range: RangeBounds<RangedUsize<0, 32767>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range);
        unsafe { Reader::new_unchecked(address, len) }
    }

    /// Returns a writer over the given range.
    pub fn writer<'a, 'b, Range>(&'a mut self, range: Range) -> Writer<'b>
    where
        Range: RangeBounds<RangedUsize<0, 32767>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range);
        unsafe { Writer::new_unchecked(address, len) }
    }
}

#[cfg(test)]
mod tests {
    use super::{translate_range_to_buffer, Error, Sram, SRAM_MEMORY};
    use claims::{assert_err_eq, assert_ok_eq};
    use deranged::RangedUsize;
    use embedded_io::{Read, Write};
    use gba_test::test;
    use more_ranges::{
        RangeFromExclusive, RangeFromExclusiveToExclusive, RangeFromExclusiveToInclusive,
    };

    #[test]
    fn translate_range_to_buffer_unbounded_unbounded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(..),
            (SRAM_MEMORY, 32768)
        );
    }

    #[test]
    fn translate_range_to_buffer_unbounded_included() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(..=RangedUsize::new_static::<42>()),
            (SRAM_MEMORY, 43)
        );
    }

    #[test]
    fn translate_range_to_buffer_unbounded_excluded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(..RangedUsize::new_static::<42>()),
            (SRAM_MEMORY, 42)
        );
    }

    #[test]
    fn translate_range_to_buffer_included_unbounded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(RangedUsize::new_static::<42>()..),
            (unsafe { SRAM_MEMORY.add(42) }, 32726)
        );
    }

    #[test]
    fn translate_range_to_buffer_included_included() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(
                RangedUsize::new_static::<42>()..=RangedUsize::new_static::<100>()
            ),
            (unsafe { SRAM_MEMORY.add(42) }, 59)
        );
    }

    #[test]
    fn translate_range_to_buffer_included_excluded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(
                RangedUsize::new_static::<42>()..RangedUsize::new_static::<100>()
            ),
            (unsafe { SRAM_MEMORY.add(42) }, 58)
        );
    }

    #[test]
    fn translate_range_to_buffer_excluded_unbounded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(RangeFromExclusive {
                start: RangedUsize::new_static::<42>()
            }),
            (unsafe { SRAM_MEMORY.add(43) }, 32725)
        );
    }

    #[test]
    fn translate_range_to_buffer_excluded_included() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(RangeFromExclusiveToInclusive {
                start: RangedUsize::new_static::<42>(),
                end: RangedUsize::new_static::<100>()
            }),
            (unsafe { SRAM_MEMORY.add(43) }, 58)
        );
    }

    #[test]
    fn translate_range_to_buffer_excluded_excluded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(RangeFromExclusiveToExclusive {
                start: RangedUsize::new_static::<42>(),
                end: RangedUsize::new_static::<100>()
            }),
            (unsafe { SRAM_MEMORY.add(43) }, 57)
        );
    }

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
