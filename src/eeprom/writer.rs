use crate::eeprom::{
    populate_address, read_bits, write, Error, ADDRESS_LEN_512B, ADDRESS_LEN_8KB, EEPROM_MEMORY,
};
use core::{cmp::min, marker::PhantomData};
use deranged::{OptionRangedUsize, RangedUsize};
use embedded_io::{ErrorType, Write};

const LONG_ADDRESS_LEN_512B: usize = ADDRESS_LEN_512B + 3;
const LONG_ADDRESS_LEN_8KB: usize = ADDRESS_LEN_8KB + 3;
const BIT_LEN_512B: usize = 67 + ADDRESS_LEN_512B;
const BIT_LEN_8KB: usize = 67 + ADDRESS_LEN_8KB;

#[derive(Debug)]
struct Writer<'a> {
    address: *mut u8,
    len: usize,
    index: OptionRangedUsize<0, 7>,
    lifetime: PhantomData<&'a ()>,
}

impl Writer<'_> {
    unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        Self {
            address,
            len,
            index: OptionRangedUsize::None,
            lifetime: PhantomData,
        }
    }

    fn write<const ADDRESS_LEN: usize, const BIT_LEN: usize, const LONG_ADDRESS_LEN: usize>(
        &mut self,
        buf: &[u8],
        bits: &mut [u16; BIT_LEN],
    ) -> Result<usize, Error> {
        // Write in chunks of 8 bytes.
        let mut write_count = 0;
        loop {
            if let Some(index) = self.index.get() {
                let write_limit = min(buf.len(), self.len);
                if write_count >= write_limit {
                    if self.len == 0 {
                        return Err(Error::EndOfWriter);
                    }
                    self.address = unsafe { self.address.add(write_count) };
                    self.len = self.len.saturating_sub(write_count);
                    return Ok(write_count);
                }

                // Populate the data to be written.
                let mut new_index = Some(index);
                for (byte, bits_group) in buf[write_count..write_limit]
                    .iter()
                    .copied()
                    .take(8 - index.get())
                    .zip(
                        bits[(2 + ADDRESS_LEN + index.get() * 8)..(66 + ADDRESS_LEN)].chunks_mut(8),
                    )
                {
                    for (i, bit) in bits_group.iter_mut().enumerate() {
                        *bit = (byte as u16 >> (7 - i)) & 1;
                    }
                    write_count += 1;
                    if let Some(index) = new_index {
                        new_index = index.checked_add(1);
                    }
                }
                self.index = new_index.into();
                if new_index.is_none() {
                    self.flush_unchecked::<ADDRESS_LEN, BIT_LEN>(bits)?;
                }
            } else {
                *bits = [0u16; BIT_LEN];
                bits[0] = 1;
                bits[1] = 0;
                // Read in any previous bits if we are not starting at zero.
                if self.address as usize & 0b0000_0111 != 0 && write_count == 0 {
                    // Request the read.
                    let mut new_bits = [0; LONG_ADDRESS_LEN];
                    new_bits[0] = 1;
                    new_bits[1] = 1;
                    populate_address::<ADDRESS_LEN>(&mut new_bits[2..], unsafe {
                        self.address.byte_add(write_count)
                    });
                    write(&new_bits);

                    // Note that we can ignore the first four bits; they'll be overwritten by the
                    // address in the next step anyway.
                    read_bits(&mut bits[(2 + ADDRESS_LEN)..]);
                }
                populate_address::<ADDRESS_LEN>(&mut bits[2..], unsafe {
                    self.address.byte_add(write_count)
                });
                if write_count == 0 {
                    self.index = OptionRangedUsize::Some(unsafe {
                        RangedUsize::new_unchecked(self.address as usize & 0b0000_0111)
                    });
                } else {
                    self.index = OptionRangedUsize::Some(RangedUsize::new_static::<0>());
                }
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
        bits: &[u16; BIT_LEN],
    ) -> Result<(), Error> {
        write(bits);
        // Wait for the write to succeed.
        for _ in 0..10000 {
            if unsafe { (EEPROM_MEMORY as *mut u16).read_volatile() } & 1 > 0 {
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

                return Ok(());
            }
        }
        Err(Error::OperationTimedOut)
    }

    fn flush<const ADDRESS_LEN: usize, const BIT_LEN: usize>(
        &mut self,
        bits: &mut [u16; BIT_LEN],
    ) -> Result<(), Error> {
        // If the address is not aligned, then we need to make sure we aren't overwriting any
        // trailing data.
        //
        // We resolve this by checking and reading any trailing data.
        if self.address as usize & 0b0000_0111 != 0 {
            let mut new_bits = [0; 68];

            // Request the read.
            new_bits[0] = 1;
            new_bits[1] = 1;
            populate_address::<ADDRESS_LEN>(&mut new_bits[2..], self.address);
            write(&new_bits);
            read_bits(&mut new_bits);

            // Figure out how many bits to copy over.
            for (bit, new_bit) in
                bits.iter_mut().zip(new_bits.iter()).rev().take(
                    ((self.address as usize & (!0b0000_0111)) + 7 - self.address as usize) * 8,
                )
            {
                *bit = *new_bit;
            }
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
        Self {
            writer: unsafe { Writer::new_unchecked(address, len) },
            bits: [0; BIT_LEN_512B],
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
        Self {
            writer: unsafe { Writer::new_unchecked(address, len) },
            bits: [0; BIT_LEN_8KB],
        }
    }
}

impl ErrorType for Writer8K<'_> {
    type Error = Error;
}

impl Write for Writer8K<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.writer
            .write::<ADDRESS_LEN_8KB, BIT_LEN_8KB, LONG_ADDRESS_LEN_8KB>(buf, &mut self.bits)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.writer
            .flush::<ADDRESS_LEN_8KB, BIT_LEN_8KB>(&mut self.bits)
    }
}

impl Drop for Writer8K<'_> {
    fn drop(&mut self) {
        // This will swallow any errors.
        let _ignored_result = self.flush();
    }
}
