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
#[derive(Debug)]
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
        Bound::Excluded(start) => start.get().saturating_sub(1),
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
