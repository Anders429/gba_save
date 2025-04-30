use crate::{
    mmio::{Cycles, DmaControl, DMA3_CNT, DMA3_DESTINATION, DMA3_LEN, DMA3_SOURCE, IME, WAITCNT},
    range::translate_range_to_buffer,
};
use core::{cmp::min, convert::Infallible, marker::PhantomData, ops::RangeBounds};
use deranged::{OptionRangedUsize, RangedUsize};
use embedded_io::{ErrorKind, ErrorType, Read, Write};

const EEPROM_MEMORY: *mut u8 = 0x0D00_0000 as *mut u8;
const ADDRESS_LEN_512B: usize = 6;
const ADDRESS_LEN_8KB: usize = 14;
const LONG_ADDRESS_LEN_512B: usize = ADDRESS_LEN_512B + 3;
const LONG_ADDRESS_LEN_8KB: usize = ADDRESS_LEN_8KB + 3;
const BIT_LEN_512B: usize = 73;
const BIT_LEN_8KB: usize = 81;

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    OperationTimedOut,
    WriteFailure,
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Writer<'a> {
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

#[derive(Debug)]
pub struct Reader512B<'a> {
    reader: Reader<'a>,
}

impl Reader512B<'_> {
    unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
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

#[derive(Debug)]
pub struct Writer512B<'a> {
    writer: Writer<'a>,
    bits: [u16; BIT_LEN_512B],
}

impl Writer512B<'_> {
    unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Reader8K<'a> {
    reader: Reader<'a>,
}

impl Reader8K<'_> {
    unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
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

#[derive(Debug)]
pub struct Writer8K<'a> {
    writer: Writer<'a>,
    bits: [u16; BIT_LEN_8KB],
}

impl Writer8K<'_> {
    unsafe fn new_unchecked(address: *mut u8, len: usize) -> Self {
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

#[derive(Debug)]
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
        let (address, len) = translate_range_to_buffer(range, EEPROM_MEMORY);
        unsafe { Reader8K::new_unchecked(address, len) }
    }

    pub fn writer<'a, 'b, Range>(&'a mut self, range: Range) -> Writer8K<'a>
    where
        Range: RangeBounds<RangedUsize<0, 8191>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range, EEPROM_MEMORY);
        unsafe { Writer8K::new_unchecked(address, len) }
    }
}

#[cfg(test)]
mod tests {
    use super::{Eeprom512B, Eeprom8K, Error};
    use claims::{assert_err_eq, assert_ok, assert_ok_eq};
    use deranged::RangedUsize;
    use embedded_io::{Read, Write};
    use gba_test::test;

    #[test]
    #[cfg_attr(
        not(eeprom_512b),
        ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable."
    )]
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
    #[cfg_attr(
        not(eeprom_512b),
        ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable."
    )]
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
    #[cfg_attr(
        not(eeprom_512b),
        ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable."
    )]
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
    #[cfg_attr(
        not(eeprom_512b),
        ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable."
    )]
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
    #[cfg_attr(
        not(eeprom_512b),
        ignore = "This test requires a 512B EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_512b` to enable."
    )]
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

    // Note that we can't test for `WriteFailure` on mGBA because mGBA automatically coerces writes
    // from 8KiB to 512B if they are the wrong size. This means we can't actually test that case in
    // mGBA, because having no EEPROM at all means we will always time out.
    #[test]
    #[cfg_attr(
        any(eeprom_512b, eeprom_8k),
        ignore = "This test cannot be run with an EEPROM chip. Ensure EEPROM is not configured and don't pass `--cfg eeprom_512b` or `--cfg eeprom_8k` to enable."
    )]
    fn timed_out_512b() {
        let mut eeprom = unsafe { Eeprom512B::new() };
        let mut writer = eeprom.writer(..);

        assert_err_eq!(writer.write(b"hello, world!"), Error::OperationTimedOut);
    }

    #[test]
    #[cfg_attr(
        not(eeprom_8k),
        ignore = "This test requires a 8KiB EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_8k` to enable."
    )]
    fn empty_range_read_8k() {
        let mut eeprom = unsafe { Eeprom8K::new() };
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
    #[cfg_attr(
        not(eeprom_8k),
        ignore = "This test requires a 8KiB EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_8k` to enable."
    )]
    fn empty_range_write_8k() {
        let mut eeprom = unsafe { Eeprom8K::new() };
        assert_err_eq!(
            eeprom
                .writer(RangedUsize::new_static::<0>()..RangedUsize::new_static::<0>())
                .write(&[0]),
            Error::EndOfWriter
        );
    }

    #[test]
    #[cfg_attr(
        not(eeprom_8k),
        ignore = "This test requires a 8KiB EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_8k` to enable."
    )]
    fn full_range_8k() {
        let mut eeprom = unsafe { Eeprom8K::new() };
        let mut writer = eeprom.writer(..);

        for i in 0..2048 {
            assert_ok_eq!(
                writer.write(&[
                    0u8.wrapping_add(i as u8),
                    1u8.wrapping_add(i as u8),
                    2u8.wrapping_add(i as u8),
                    3u8.wrapping_add(i as u8),
                ]),
                4,
                "i = {i}",
            );
        }

        let mut reader = eeprom.reader(..);
        let mut buf = [0; 4];

        for i in 0..2048 {
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
    #[cfg_attr(
        not(eeprom_8k),
        ignore = "This test requires a 8KiB EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_8k` to enable."
    )]
    fn partial_range_8k() {
        let mut eeprom = unsafe { Eeprom8K::new() };
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
    #[cfg_attr(
        not(eeprom_8k),
        ignore = "This test requires a 8KiB EEPROM chip. Ensure EEPROM is configured and pass `--cfg eeprom_8k` to enable."
    )]
    fn offset_8k() {
        let mut eeprom = unsafe { Eeprom8K::new() };
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

    // Note that we can't test for `WriteFailure` on mGBA because mGBA automatically coerces writes
    // from 512B to 8KiB if they are the wrong size. This means we can't actually test that case in
    // mGBA, because having no EEPROM at all means we will always time out.
    #[test]
    #[cfg_attr(
        any(eeprom_512b, eeprom_8k),
        ignore = "This test cannot be run with an EEPROM chip. Ensure EEPROM is not configured and don't pass `--cfg eeprom_512b` or `--cfg eeprom_8k` to enable."
    )]
    fn timed_out_8k() {
        let mut eeprom = unsafe { Eeprom8K::new() };
        let mut writer = eeprom.writer(..);

        assert_err_eq!(writer.write(b"hello, world!"), Error::OperationTimedOut);
    }
}
