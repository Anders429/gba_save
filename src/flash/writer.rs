use crate::{
    flash::{
        Bank, Command, Error, FLASH_MEMORY, Reader64K, SIZE_64KB, send_command, switch_bank,
        verify_byte, verify_bytes,
    },
    log,
    mmio::IME,
};
use core::{cmp::min, marker::PhantomData, ptr, time::Duration};
use embedded_io::{ErrorType, Read, Write};

/// A writer on a 64KiB flash device.
///
/// This type allows writing data on the range specified upon creation.
///
/// If the memory being written to has been written to previously without being erased, the writes
/// will not succeed.
#[derive(Debug)]
pub struct Writer64K<'a> {
    address: *mut u8,
    len: usize,
    lifetime: PhantomData<&'a ()>,
}

impl Writer64K<'_> {
    pub(crate) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        log::info!(
            "Creating Flash 64KiB writer at address 0x{:08x?} with length {len}",
            address as usize
        );
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

/// A writer on a 128KiB flash device.
///
/// This type allows writing data on the range specified upon creation.
///
/// If the memory being written to has been written to previously without being erased, the writes
/// will not succeed.
#[derive(Debug)]
pub struct Writer128K<'a> {
    address: *mut u8,
    len: usize,
    bank: Bank,
    lifetime: PhantomData<&'a ()>,
}

impl Writer128K<'_> {
    pub(crate) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        log::info!(
            "Creating Flash 128KiB writer at address 0x{:08x?} with length {len}",
            address as usize
        );
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

/// A writer on a 64KiB Atmel flash device.
///
/// This type allows writing data on the range specified upon creation.
#[derive(Debug)]
pub struct Writer64KAtmel<'a> {
    address: *mut u8,
    len: usize,
    buf: [u8; 128],
    flushed: bool,
    lifetime: PhantomData<&'a ()>,
}

impl Writer64KAtmel<'_> {
    pub(crate) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        log::info!(
            "Creating Flash 64KiB Atmel writer at address 0x{:08x?} with length {len}",
            address as usize
        );
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
                self.len -= write_count;
                return Ok(write_count);
            }

            unsafe {
                *self.buf.get_unchecked_mut(self.address as usize % 128) =
                    *buf.get_unchecked(write_count);
            }
            self.flushed = false;

            unsafe { self.address = self.address.add(1) };

            if self.address as usize % 128 == 0 {
                self.flush()?;
            }

            write_count += 1;
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        if self.flushed {
            return Ok(());
        }

        self.flushed = true;

        // Read any remaining bytes at the back of the buffer.
        let offset = self.address as usize % 128;
        if offset != 0 {
            let mut reader = unsafe { Reader64K::new_unchecked(self.address, 128 - offset) };
            unsafe {
                reader
                    .read_exact(self.buf.get_unchecked_mut(offset..))
                    .unwrap_unchecked()
            };
        }

        let offset_address = unsafe { self.address.sub(if offset == 0 { 128 } else { offset }) };

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

        Ok(())
    }
}

impl Drop for Writer64KAtmel<'_> {
    fn drop(&mut self) {
        if !self.flushed {
            log::warn!(
                "Dropped Flash Atmel 64KiB writer without flushing remaining {} bytes. They will be flushed automatically, but any errors will not be handled.",
                self.address as usize % 128
            );
        }
        // This will swallow any errors.
        let _ignored_result = self.flush();
    }
}
