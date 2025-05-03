use crate::{log, sram::Error};
use core::{cmp::min, marker::PhantomData};
use embedded_io::{ErrorType, Write};

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
#[derive(Debug)]
pub struct Writer<'a> {
    address: *mut u8,
    len: usize,
    lifetime: PhantomData<&'a ()>,
}

impl Writer<'_> {
    pub(in crate::sram) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        log::info!(
            "Creating SRAM writer at address 0x{:08x?} with length {len}",
            address as usize
        );
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
