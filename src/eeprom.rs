use crate::{
    mmio::{Cycles, DmaControl, DMA3_CNT, DMA3_DESTINATION, DMA3_LEN, DMA3_SOURCE, IME, WAITCNT},
    range::translate_range_to_buffer,
};
use core::{cmp::min, convert::Infallible, marker::PhantomData, ops::RangeBounds};
use deranged::{OptionRangedUsize, RangedUsize};
use embedded_io::{ErrorKind, ErrorType, Read, Write};

const EEPROM_MEMORY: *mut u8 = 0x0D00_0000 as *mut u8;
const ADDRESS_512B: usize = 6;
const ADDRESS_8KB: usize = 14;

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    Timeout,
    EndOfWriter,
}

impl embedded_io::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            _ => todo!(),
        }
    }
}

// Interacting with EEPROM works essentially in "sectors" of 8 bytes. Therefore, when writing and
// reading data, we need to offset based on the actual address we want to access and ensure we
// don't overwrite other data with 0s accidentally.

fn write(bits: &[u16]) {
    unsafe {
        // Disable interrupts.
        let previous_ime = IME.read_volatile();
        IME.write_volatile(false);

        // Write bits using DMA3.
        let mut waitcnt = WAITCNT.read_volatile();
        waitcnt.set_eeprom_waitstate(Cycles::_8);
        WAITCNT.write_volatile(waitcnt);

        DMA3_DESTINATION.write_volatile(EEPROM_MEMORY as *mut u16);
        DMA3_SOURCE.write_volatile(bits.as_ptr());
        DMA3_LEN.write_volatile(bits.len() as u16);
        DMA3_CNT.write_volatile(DmaControl::new().enable());

        // Wait for write to finish.
        while DMA3_CNT.read_volatile().enabled() {}

        // Re-enable interrupts.
        IME.write_volatile(previous_ime);
    }
}

fn read_bits(buf: &mut [u16]) {
    unsafe {
        // Disable interrupts.
        let previous_ime = IME.read_volatile();
        IME.write_volatile(false);

        let mut waitcnt = WAITCNT.read_volatile();
        waitcnt.set_eeprom_waitstate(Cycles::_8);
        WAITCNT.write_volatile(waitcnt);

        // Read bits using DMA3.
        DMA3_DESTINATION.write_volatile(buf.as_mut_ptr());
        DMA3_SOURCE.write_volatile(EEPROM_MEMORY as *mut u16);
        DMA3_LEN.write_volatile(68);
        DMA3_CNT.write_volatile(DmaControl::new().enable());

        // Wait for read to finish.
        while DMA3_CNT.read_volatile().enabled() {}

        // Re-enable interrupts.
        IME.write_volatile(previous_ime);
    }
}

fn read(mut bit_buffer: [u16; 68], output_buffer: &mut [u8], offset: RangedUsize<0, 7>) {
    read_bits(&mut bit_buffer);

    // Now we write the bits to the output buffer.
    for (bits, byte) in bit_buffer[(4 + 8 * offset.get())..]
        .chunks(8)
        .zip(output_buffer.iter_mut().take(8 - offset.get()))
    {
        *byte = 0;
        for (i, bit) in bits.iter().copied().enumerate() {
            *byte |= (bit as u8 & 1) << (7 - i)
        }
    }
}

/// Populate an address to a bit buffer to be manipulated on the EEPROM.
///
/// Note that this only populates with an alignment of 8. Callers should make sure they account for
/// the alignment.
///
/// Pass in an appropraite `ADDRESS_LEN` depending on the type of chip being interracted with. 8KiB
/// chips use a value of 14, and 512B chips use a value of 6.
fn populate_address<const ADDRESS_LEN: usize>(bit_buffer: &mut [u16], address: *mut u8) {
    for i in 0..ADDRESS_LEN {
        let shift = ADDRESS_LEN - 1 - i;
        bit_buffer[i] = ((address as usize >> (shift + 3)) & 1) as u16;
    }
}

pub struct Reader512B<'a> {
    address: *mut u8,
    len: usize,
    lifetime: PhantomData<&'a ()>,
}

impl Reader512B<'_> {
    unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        Self {
            address,
            len,
            lifetime: PhantomData,
        }
    }
}

impl ErrorType for Reader512B<'_> {
    type Error = Infallible;
}

impl Read for Reader512B<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
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
            populate_address::<ADDRESS_512B>(&mut bits[2..], unsafe {
                self.address.byte_add(read_count)
            });

            // Send to EEPROM
            write(&bits[..9]);
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

#[derive(Debug)]
pub struct Writer512B<'a> {
    address: *mut u8,
    len: usize,
    bits: [u16; 73],
    index: OptionRangedUsize<0, 7>,
    lifetime: PhantomData<&'a ()>,
}

impl Writer512B<'_> {
    unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
        Self {
            address,
            len,
            bits: [0; 73],
            index: OptionRangedUsize::None,
            lifetime: PhantomData,
        }
    }

    /// Flush without checking the current address and populating extra data.
    ///
    /// Despite the name, this isn't actually unsafe. It just indicates that data might be
    /// overwritten in memory if the internal buffer isn't completely aligned.
    fn flush_unchecked(&mut self) -> Result<(), Error> {
        write(&self.bits);
        // Wait for the write to succeed.
        for _ in 0..10000 {
            if unsafe { (EEPROM_MEMORY as *mut u16).read_volatile() } & 1 > 0 {
                return Ok(());
            }
        }
        Err(Error::Timeout)
    }
}

impl ErrorType for Writer512B<'_> {
    type Error = Error;
}

impl Write for Writer512B<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
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
                    log::debug!("{:?}", self);
                    return Ok(write_count);
                }

                // Populate the data to be written.
                let mut new_index = Some(index);
                for (byte, bits) in buf[write_count..write_limit]
                    .iter()
                    .copied()
                    .take(8 - index.get())
                    .zip(self.bits[8 + (index.get() * 8)..72].chunks_mut(8))
                {
                    for (i, bit) in bits.iter_mut().enumerate() {
                        *bit = (byte as u16 >> (7 - i)) & 1;
                    }
                    write_count += 1;
                    if let Some(index) = new_index {
                        new_index = index.checked_add(1);
                    }
                }
                self.index = new_index.into();
                if new_index.is_none() {
                    self.flush_unchecked()?;
                }
            } else {
                self.bits = [0u16; 73];
                self.bits[0] = 1;
                self.bits[1] = 0;
                // Read in any previous bits if we are not starting at zero.
                if self.address as usize & 0b0000_0111 != 0 && write_count == 0 {
                    // Request the read.
                    let mut bits = [0; 9];
                    bits[0] = 1;
                    bits[1] = 1;
                    populate_address::<ADDRESS_512B>(&mut bits[2..], unsafe {
                        self.address.byte_add(write_count)
                    });
                    write(&bits);

                    // Note that we can ignore the first four bits; they'll be overwritten by the
                    // address in the next step anyway.
                    read_bits(&mut self.bits[4..]);
                }
                populate_address::<ADDRESS_512B>(&mut self.bits[2..], unsafe {
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

    fn flush(&mut self) -> Result<(), Self::Error> {
        // If the address is not aligned, then we need to make sure we aren't overwriting any
        // trailing data.
        //
        // We resolve this by checking and reading any trailing data.
        if self.address as usize & 0b0000_0111 != 0 {
            let mut bits = [0; 68];

            // Request the read.
            bits[0] = 1;
            bits[1] = 1;
            populate_address::<ADDRESS_512B>(&mut bits[2..], self.address);
            write(&bits);
            read_bits(&mut bits);

            // Figure out how many bits to copy over.
            for (bit, new_bit) in
                self.bits.iter_mut().zip(bits.iter()).rev().take(
                    ((self.address as usize & (!0b0000_0111)) + 7 - self.address as usize) * 8,
                )
            {
                *bit = *new_bit;
            }
        }

        self.flush_unchecked()
    }
}

pub struct Eeprom512B {
    _private: (),
}

impl Eeprom512B {
    pub unsafe fn new() -> Self {
        Self { _private: () }
    }

    pub fn reader<'a, 'b, Range>(&'a mut self, range: Range) -> Reader512B<'a>
    where
        Range: RangeBounds<RangedUsize<0, 511>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range, EEPROM_MEMORY);
        unsafe { Reader512B::new_unchecked(address, len) }
    }

    pub fn writer<'a, 'b, Range>(&'a mut self, range: Range) -> Writer512B<'a>
    where
        Range: RangeBounds<RangedUsize<0, 511>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range, EEPROM_MEMORY);
        unsafe { Writer512B::new_unchecked(address, len) }
    }
}

pub struct Reader8K<'a> {
    lifetime: PhantomData<&'a ()>,
}

pub struct Writer8K<'a> {
    lifetime: PhantomData<&'a ()>,
}

pub struct Eeprom8K {
    _private: (),
}

impl Eeprom8K {
    pub unsafe fn new() -> Self {
        Self { _private: () }
    }

    pub fn reader<'a, 'b, Range>(&'a mut self, range: Range) -> Reader8K<'a>
    where
        Range: RangeBounds<RangedUsize<0, 8191>>,
        'a: 'b,
    {
        todo!()
    }

    pub fn writer<'a, 'b, Range>(&'a mut self, range: Range) -> Writer8K<'a>
    where
        Range: RangeBounds<RangedUsize<0, 8191>>,
        'a: 'b,
    {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::{Eeprom512B, Error};
    use claims::{assert_err_eq, assert_ok, assert_ok_eq};
    use deranged::RangedUsize;
    use embedded_io::{Read, Write};
    use gba_test::test;

    #[test]
    #[cfg_attr(not(eeprom_512b), ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable.")]
    fn empty_range_read_512b() {
        let mut eeprom = unsafe { Eeprom512B::new() };
        let mut buf = [1, 2, 3, 4];

        assert_ok_eq!(
            eeprom
                .reader(RangedUsize::new_static::<0>()..RangedUsize::new_static::<0>())
                .read(&mut buf),
            0
        );
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    #[test]
    #[cfg_attr(not(eeprom_512b), ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable.")]
    fn empty_range_write_512b() {
        let mut eeprom = unsafe { Eeprom512B::new() };
        assert_err_eq!(
            eeprom
                .writer(RangedUsize::new_static::<0>()..RangedUsize::new_static::<0>())
                .write(&[0]),
            Error::EndOfWriter
        );
    }

    #[test]
    #[cfg_attr(not(eeprom_512b), ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable.")]
    fn full_range_512b() {
        let mut eeprom = unsafe { Eeprom512B::new() };
        let mut writer = eeprom.writer(..);

        for i in 0..128 {
            assert_ok_eq!(
                writer.write(&[
                    0u8.wrapping_add(i as u8),
                    1u8.wrapping_add(i as u8),
                    2u8.wrapping_add(i as u8),
                    3u8.wrapping_add(i as u8),
                ]),
                4
            );
        }

        let mut reader = eeprom.reader(..);
        let mut buf = [0; 4];

        for i in 0..128 {
            assert_ok_eq!(reader.read(&mut buf), 4);
            assert_eq!(
                buf,
                [
                    0u8.wrapping_add(i as u8),
                    1u8.wrapping_add(i as u8),
                    2u8.wrapping_add(i as u8),
                    3u8.wrapping_add(i as u8),
                ],
                "i = {i}"
            );
        }
    }

    #[test]
    #[cfg_attr(not(eeprom_512b), ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable.")]
    fn partial_range_512b() {
        let mut eeprom = unsafe { Eeprom512B::new() };
        let mut writer =
            eeprom.writer(RangedUsize::new_static::<42>()..RangedUsize::new_static::<100>());

        assert_ok_eq!(writer.write(&[b'a'; 100]), 58);
        assert_ok!(writer.flush());

        let mut reader =
            eeprom.reader(RangedUsize::new_static::<51>()..RangedUsize::new_static::<60>());
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
    #[cfg_attr(not(eeprom_512b), ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable.")]
    fn offset_512b() {
        let mut eeprom = unsafe { Eeprom512B::new() };
        let mut writer =
            eeprom.writer(RangedUsize::new_static::<4>()..RangedUsize::new_static::<7>());

        assert_ok_eq!(writer.write(b"abc"), 3);
        assert_ok!(writer.flush());

        let mut reader =
            eeprom.reader(RangedUsize::new_static::<4>()..RangedUsize::new_static::<7>());
        let mut buf = [0; 3];

        assert_ok_eq!(reader.read(&mut buf), 3);
        assert_eq!(&buf, b"abc");
    }
}
