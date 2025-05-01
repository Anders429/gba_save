use core::{cmp::min, convert::Infallible, marker::PhantomData};
use embedded_io::{ErrorType, Read};

/// A reader on SRAM.
///
/// This type allows reading data over the range specified upon creation.
#[derive(Debug)]
pub struct Reader<'a> {
    address: *mut u8,
    len: usize,
    lifetime: PhantomData<&'a ()>,
}

impl Reader<'_> {
    pub(in crate::sram) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
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
