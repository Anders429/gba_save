use crate::{
    flash::{
        send_command, switch_bank, verify_byte, verify_bytes, Bank, Command, Error, Reader64K,
        FLASH_MEMORY, SIZE_64KB,
    },
    mmio::IME,
};
use core::{cmp::min, marker::PhantomData, ptr, time::Duration};
use embedded_io::{ErrorType, Read, Write};

pub struct Writer64K<'a> {
    address: *mut u8,
    len: usize,
    lifetime: PhantomData<&'a ()>,
}

impl Writer64K<'_> {
    pub(crate) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        Self {
            address,
            len,
            lifetime: PhantomData,
        }
    }
}

impl ErrorType for Writer64K<'_> {
    type Error = Error;
}

impl Write for Writer64K<'_> {
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
            send_command(Command::Write);
            unsafe {
                address.write_volatile(byte);
            }
            verify_byte(address, byte, Duration::from_millis(20))?;

            write_count += 1;
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub struct Writer128K<'a> {
    address: *mut u8,
    len: usize,
    bank: Bank,
    lifetime: PhantomData<&'a ()>,
}

impl Writer128K<'_> {
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

impl ErrorType for Writer128K<'_> {
    type Error = Error;
}

impl Write for Writer128K<'_> {
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

            let mut address = unsafe { self.address.add(write_count) };
            if matches!(self.bank, Bank::_0) {
                if ptr::eq(address, unsafe { FLASH_MEMORY.add(SIZE_64KB) }) {
                    self.bank = Bank::_1;
                    switch_bank(self.bank);
                }
            }
            if matches!(self.bank, Bank::_1) {
                address = unsafe { address.sub(SIZE_64KB) };
            }

            let byte = unsafe { *buf.get_unchecked(write_count) };
            send_command(Command::Write);
            unsafe {
                address.write_volatile(byte);
            }
            verify_byte(address, byte, Duration::from_millis(20))?;

            write_count += 1;
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub struct Writer64KAtmel<'a> {
    address: *mut u8,
    len: usize,
    buf: [u8; 128],
    flushed: bool,
    lifetime: PhantomData<&'a ()>,
}

impl Writer64KAtmel<'_> {
    pub(crate) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        let mut buf = [0xff; 128];
        let mut flushed = true;

        // Read data in case of unalignment.
        let offset = address.align_offset(128);
        if offset != 0 {
            let mut reader = unsafe { Reader64K::new_unchecked(address.sub(offset), offset) };
            unsafe {
                reader
                    .read_exact(buf.get_unchecked_mut(..offset))
                    .unwrap_unchecked()
            };
            flushed = false;
        }

        Self {
            address,
            len,
            buf,
            flushed,
            lifetime: PhantomData,
        }
    }
}

impl ErrorType for Writer64KAtmel<'_> {
    type Error = Error;
}

impl Write for Writer64KAtmel<'_> {
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

            let offset = self.address.align_offset(128);
            unsafe {
                *self.buf.get_unchecked_mut(offset) = *buf.get_unchecked(write_count);
            }
            self.flushed = false;

            if offset >= 127 {
                self.flush()?;
            }

            write_count += 1;
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        if self.flushed {
            return Ok(());
        }

        // Read any remaining bytes at the back of the buffer.
        let offset = self.address.align_offset(128);
        let remaining = 128 - offset;
        if remaining != 128 {
            let mut reader = unsafe { Reader64K::new_unchecked(self.address, remaining) };
            unsafe {
                reader
                    .read_exact(self.buf.get_unchecked_mut(offset..))
                    .unwrap_unchecked()
            };
        }

        let offset_address = unsafe { self.address.sub(offset) };

        // Disable interrupts, storing the previous value.
        //
        // This prevents anything from interrupting during writes to memory. GBATEK recommends
        // disabling interrupts on writes to Atmel devices.
        let previous_ime = unsafe { IME.read_volatile() };
        // SAFETY: This is guaranteed to be a valid write.
        unsafe { IME.write_volatile(false) };

        send_command(Command::Write);
        for (i, &byte) in self.buf.iter().enumerate() {
            unsafe { offset_address.add(i).write_volatile(byte) };
        }

        // Restore previous interrupt enable value.
        // SAFETY: This is guaranteed to be a valid write.
        unsafe {
            IME.write_volatile(previous_ime);
        }

        verify_bytes(offset_address, &self.buf, Duration::from_millis(20))?;

        self.flushed = true;
        Ok(())
    }
}

impl Drop for Writer64KAtmel<'_> {
    fn drop(&mut self) {
        // This will swallow any errors.
        let _ignored_result = self.flush();
    }
}
