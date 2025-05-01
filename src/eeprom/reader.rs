use crate::eeprom::{populate_address, read, write, ADDRESS_LEN_512B, ADDRESS_LEN_8KB};
use core::{cmp::min, convert::Infallible, marker::PhantomData};
use deranged::RangedUsize;
use embedded_io::{ErrorType, Read};

#[derive(Debug)]
struct Reader<'a> {
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

    fn read<const ADDRESS_LEN: usize>(&mut self, buf: &mut [u8]) -> Result<usize, Infallible> {
        let mut bits = [0u16; 68];

        // Read in chunks of 8 bytes.
        let mut read_count = 0;
        loop {
            let read_limit = min(buf.len(), self.len);
            if read_count >= read_limit {
                self.address = unsafe { self.address.add(read_count) };
                self.len -= read_count;
                return Ok(read_count);
            }

            // Request a read of EEPROM data.
            bits[0] = 1;
            bits[1] = 1;
            populate_address::<ADDRESS_LEN>(&mut bits[2..], unsafe {
                self.address.byte_add(read_count)
            });

            // Send to EEPROM
            write(&bits[..(ADDRESS_LEN + 3)]);
            // Receive from EEPROM.
            let bits_to_read = read_limit - read_count;
            let offset = unsafe { RangedUsize::new_unchecked(self.address as usize & 0b0000_0111) };
            if bits_to_read < (8 - offset.get()) {
                read(
                    bits,
                    &mut buf[read_count..(read_count + bits_to_read)],
                    offset,
                );
            } else {
                read(bits, &mut buf[read_count..], offset);
            }

            read_count += min(8 - offset.get(), bits_to_read);
        }
    }
}

/// A reader on a 512B EEPROM device.
///
/// This type allows reading data over the range specified upon creation.
#[derive(Debug)]
pub struct Reader512B<'a> {
    reader: Reader<'a>,
}

impl Reader512B<'_> {
    pub(in crate::eeprom) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        Self {
            reader: unsafe { Reader::new_unchecked(address, len) },
        }
    }
}

impl ErrorType for Reader512B<'_> {
    type Error = Infallible;
}

impl Read for Reader512B<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.reader.read::<ADDRESS_LEN_512B>(buf)
    }
}

/// A reader on an 8KiB EEPROM device.
///
/// This type allows reading data over the range specified upon creation.
#[derive(Debug)]
pub struct Reader8K<'a> {
    reader: Reader<'a>,
}

impl Reader8K<'_> {
    pub(in crate::eeprom) unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        Self {
            reader: unsafe { Reader::new_unchecked(address, len) },
        }
    }
}

impl ErrorType for Reader8K<'_> {
    type Error = Infallible;
}

impl Read for Reader8K<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.reader.read::<ADDRESS_LEN_8KB>(buf)
    }
}
