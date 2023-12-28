use crate::flash::{switch_bank, Bank, FLASH_MEMORY, SIZE_64KB};
use core::{cmp::min, convert::Infallible, marker::PhantomData, ptr};
use embedded_io::{ErrorType, Read};

/// A reader on a 64KiB flash device.
///
/// This type allows reading data over the range specified upon creation.
#[derive(Debug)]
pub struct Reader64K<'a> {
    address: *mut u8,
    len: usize,
    lifetime: PhantomData<&'a ()>,
}

impl Reader64K<'_> {
    pub(crate) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        Self {
            address,
            len,
            lifetime: PhantomData,
        }
    }
}

impl ErrorType for Reader64K<'_> {
    type Error = Infallible;
}

impl Read for Reader64K<'_> {
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

/// A reader on a 128KiB flash device.
///
/// This type allows reading data over the range specified upon creation.
#[derive(Debug)]
pub struct Reader128K<'a> {
    address: *mut u8,
    len: usize,
    bank: Bank,
    lifetime: PhantomData<&'a ()>,
}

impl Reader128K<'_> {
    pub(crate) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        let bank = if address < unsafe { FLASH_MEMORY.add(SIZE_64KB) } {
            Bank::_0
        } else {
            Bank::_1
        };
        switch_bank(bank);

        Self {
            address,
            len,
            bank,
            lifetime: PhantomData,
        }
    }
}

impl ErrorType for Reader128K<'_> {
    type Error = Infallible;
}

impl Read for Reader128K<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let mut read_count = 0;
        loop {
            if read_count >= min(buf.len(), self.len) {
                self.address = unsafe { self.address.add(read_count) };
                self.len -= read_count;
                return Ok(read_count);
            }

            let mut address = unsafe { self.address.add(read_count) };
            if matches!(self.bank, Bank::_0) {
                if ptr::eq(address, unsafe { FLASH_MEMORY.add(SIZE_64KB) }) {
                    self.bank = Bank::_1;
                    switch_bank(self.bank);
                }
            }
            if matches!(self.bank, Bank::_1) {
                address = unsafe { address.sub(SIZE_64KB) };
            }

            unsafe {
                *buf.get_unchecked_mut(read_count) = address.read_volatile();
            }
            read_count += 1;
        }
    }
}
