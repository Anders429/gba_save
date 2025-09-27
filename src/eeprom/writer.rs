use crate::{
    eeprom::{
        ADDRESS_LEN_8KB, ADDRESS_LEN_512B, EEPROM_ACCESS, Error, populate_address, read_bits, write,
    },
    log,
};
use core::{cmp::min, marker::PhantomData};
use embedded_io::{ErrorType, Write};

const LONG_ADDRESS_LEN_512B: usize = ADDRESS_LEN_512B + 3;
const LONG_ADDRESS_LEN_8KB: usize = ADDRESS_LEN_8KB + 3;
const BIT_LEN_512B: usize = 67 + ADDRESS_LEN_512B;
const BIT_LEN_8KB: usize = 67 + ADDRESS_LEN_8KB;

#[derive(Debug)]
struct Writer<'a> {
    address: *mut u8,
    len: usize,
    dirty: bool,
    lifetime: PhantomData<&'a ()>,
}

impl Writer<'_> {
    unsafe fn new_unchecked<
        const ADDRESS_LEN: usize,
        const BIT_LEN: usize,
        const LONG_ADDRESS_LEN: usize,
    >(
        address: *mut u8,
        len: usize,
        bits: &mut [u16; BIT_LEN],
    ) -> Self {
        // Read in any previous bits if we are not starting at an 8-byte boundary.
        if (address as usize) & 0b0000_0111 != 0 {
            let mut read_request = [0; LONG_ADDRESS_LEN];
            read_request[0] = 1;
            read_request[1] = 1;
            populate_address::<ADDRESS_LEN>(&mut read_request[2..], address);
            write(&read_request);

            // Note that we can ignore the first four bits; they'll be overwritten by the address.
            read_bits(&mut bits[(2 + ADDRESS_LEN - 4)..]);
        }

        bits[0] = 1;
        bits[1] = 0;
        populate_address::<ADDRESS_LEN>(&mut bits[2..], address);

        Self {
            address,
            len,
            dirty: false,
            lifetime: PhantomData,
        }
    }

    fn write<const ADDRESS_LEN: usize, const BIT_LEN: usize, const LONG_ADDRESS_LEN: usize>(
        &mut self,
        buf: &[u8],
        bits: &mut [u16; BIT_LEN],
    ) -> Result<usize, Error> {
        let mut write_count = 0;
        let write_limit = min(buf.len(), self.len);
        loop {
            if write_count >= write_limit {
                if write_count == 0 && self.len == 0 {
                    return Err(Error::EndOfWriter);
                } else {
                    return Ok(write_count);
                }
            }

            // Write the data to the internal buffer.
            let offset = (self.address as usize) & 0b0000_0111;
            for (byte, bits_group) in buf[write_count..write_limit]
                .iter()
                .copied()
                .take(8 - offset)
                .zip(bits[(2 + ADDRESS_LEN + offset * 8)..(66 + ADDRESS_LEN)].chunks_mut(8))
            {
                for (i, bit) in bits_group.iter_mut().enumerate() {
                    *bit = (byte as u16 >> (7 - i)) & 1;
                }
                write_count += 1;
                self.address = unsafe { self.address.byte_add(1) };
                self.len -= 1;
                self.dirty = true;
            }
            if (self.address as usize) & 0b0000_0111 == 0 {
                self.flush_unchecked::<ADDRESS_LEN, BIT_LEN>(bits)?;
            }
        }
    }

    /// Flush without checking the current address and populating extra data.
    ///
    /// Despite the name, this isn't actually unsafe. It just indicates that data might be
    /// overwritten in memory if the internal buffer isn't completely aligned.
    ///
    /// This also performs data validation, ensuring that the EEPROM returns a "Ready" status and
    /// that the data written is correct.
    fn flush_unchecked<const ADDRESS_LEN: usize, const BIT_LEN: usize>(
        &mut self,
        bits: &mut [u16; BIT_LEN],
    ) -> Result<(), Error> {
        write(bits);
        self.dirty = false;
        // Wait for the write to succeed.
        for _ in 0..10000 {
            if unsafe { (EEPROM_ACCESS as *mut u16).read_volatile() } & 1 > 0 {
                // Verify the write.
                let mut new_bits = [0; 68];
                new_bits[0] = 1;
                new_bits[1] = 1;
                // Copy over the address that was written.
                for i in 0..ADDRESS_LEN {
                    new_bits[2 + i] = bits[2 + i];
                }
                write(&new_bits[..(ADDRESS_LEN + 3)]);
                read_bits(&mut new_bits);
                if bits[(2 + ADDRESS_LEN)..(BIT_LEN - 1)] != new_bits[4..] {
                    return Err(Error::WriteFailure);
                }

                // Populate the new address before completing.
                populate_address::<ADDRESS_LEN>(&mut bits[2..], self.address);

                return Ok(());
            }
        }
        Err(Error::OperationTimedOut)
    }

    fn flush<const ADDRESS_LEN: usize, const BIT_LEN: usize>(
        &mut self,
        bits: &mut [u16; BIT_LEN],
    ) -> Result<(), Error> {
        // Check whether we actually have any data to write.
        if (self.address as usize) & 0b0000_0111 == 0 || !self.dirty {
            return Ok(());
        }

        // Since a flush will only be executed here if the address is not aligned, we will have
        // trailing data that must be preserved.
        //
        // We resolve this by reading any trailing data.
        let mut read_request = [0; 68];
        read_request[0] = 1;
        read_request[1] = 1;
        populate_address::<ADDRESS_LEN>(&mut read_request[2..], self.address);
        write(&read_request[..(ADDRESS_LEN + 3)]);
        read_bits(&mut read_request);

        // Copy bits over.
        for (bit, new_bit) in bits[(2 + ADDRESS_LEN)..(BIT_LEN - 1)]
            .iter_mut()
            .zip(read_request[4..].iter())
            .rev()
            .take((8 - ((self.address as usize) & 0b0000_0111)) * 8)
        {
            *bit = *new_bit;
        }

        self.flush_unchecked::<ADDRESS_LEN, BIT_LEN>(bits)
    }
}

/// A writer on a 512B EEPROM device.
///
/// This type allows writing data on the range specified upon creation.
#[derive(Debug)]
pub struct Writer512B<'a> {
    writer: Writer<'a>,
    bits: [u16; BIT_LEN_512B],
}

impl Writer512B<'_> {
    pub(in crate::eeprom) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        log::info!(
            "Creating EEPROM 512B writer at address 0x{:08x?} with length {len}",
            address as usize
        );
        let mut bits = [0; BIT_LEN_512B];
        // let mut bits =
        Self {
            writer: unsafe {
                Writer::new_unchecked::<ADDRESS_LEN_512B, BIT_LEN_512B, LONG_ADDRESS_LEN_512B>(
                    address, len, &mut bits,
                )
            },
            bits,
        }
    }
}

impl ErrorType for Writer512B<'_> {
    type Error = Error;
}

impl Write for Writer512B<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.writer
            .write::<ADDRESS_LEN_512B, BIT_LEN_512B, LONG_ADDRESS_LEN_512B>(buf, &mut self.bits)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.writer
            .flush::<ADDRESS_LEN_512B, BIT_LEN_512B>(&mut self.bits)
    }
}

impl Drop for Writer512B<'_> {
    fn drop(&mut self) {
        if self.writer.dirty {
            log::warn!(
                "Dropped EEPROM 512B writer without flushing remaining {} bytes. They will be flushed automatically, but any errors will not be handled.",
                (self.writer.address as usize) & 0b0000_0111
            );
        }
        // This will swallow any errors.
        let _ignored_result = self.flush();
    }
}

/// A writer on an 8KiB EEPROM device.
///
/// This type allows writing data on the range specified upon creation.
#[derive(Debug)]
pub struct Writer8K<'a> {
    writer: Writer<'a>,
    bits: [u16; BIT_LEN_8KB],
}

impl Writer8K<'_> {
    pub(in crate::eeprom) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        log::info!(
            "Creating EEPROM 8KiB writer at address 0x{:08x?} with length {len}",
            address as usize
        );
        let mut bits = [0; BIT_LEN_8KB];
        Self {
            writer: unsafe {
                Writer::new_unchecked::<ADDRESS_LEN_8KB, BIT_LEN_8KB, LONG_ADDRESS_LEN_8KB>(
                    address, len, &mut bits,
                )
            },
            bits,
        }
    }
}

impl ErrorType for Writer8K<'_> {
    type Error = Error;
}

impl Write for Writer8K<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let val = self
            .writer
            .write::<ADDRESS_LEN_8KB, BIT_LEN_8KB, LONG_ADDRESS_LEN_8KB>(buf, &mut self.bits);
        val
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.writer
            .flush::<ADDRESS_LEN_8KB, BIT_LEN_8KB>(&mut self.bits)
    }
}

impl Drop for Writer8K<'_> {
    fn drop(&mut self) {
        if self.writer.dirty {
            log::warn!(
                "Dropped EEPROM 8KiB writer without flushing remaining {} bytes. They will be flushed automatically, but any errors will not be handled.",
                (self.writer.address as usize) & 0b0000_0111
            );
        }
        // This will swallow any errors.
        let _ignored_result = self.flush();
    }
}
