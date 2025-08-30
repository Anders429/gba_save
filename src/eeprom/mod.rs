//! EEPROM backup memory.
//!
//! The GBA has two variants of EEPROM backup:
//! - 512B
//! - 8KiB
//!
//! The methods for writing to and reading from these variants differs, so you should be deliberate
//! about which one you use. **Note**: popular emulators such as mGBA will allow writes intended
//! for one device type to be used on the other; this will not be the case on real hardware.

mod error;
mod reader;
mod writer;

pub use error::Error;
pub use reader::{Reader512B, Reader8K};
pub use writer::{Writer512B, Writer8K};

use crate::{
    mmio::{Cycles, DmaControl, DMA3_CNT, DMA3_DESTINATION, DMA3_LEN, DMA3_SOURCE, IME, WAITCNT},
    range::translate_range_to_buffer,
};
use core::ops::RangeBounds;
use deranged::RangedUsize;

const EEPROM_MEMORY: *mut u8 = 0x0D00_0000 as *mut u8;
const EEPROM_ACCESS: *mut u8 = 0x0DFF_FF00 as *mut u8;
const ADDRESS_LEN_512B: usize = 6;
const ADDRESS_LEN_8KB: usize = 14;

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

        DMA3_DESTINATION.write_volatile(EEPROM_ACCESS as *mut u16);
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
        DMA3_SOURCE.write_volatile(EEPROM_ACCESS as *mut u16);
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

/// An EEPROM device with 512B of storage.
#[derive(Debug)]
pub struct Eeprom512B {
    _private: (),
}

impl Eeprom512B {
    /// Creates an accessor to the EEPROM 512B backup memory.
    ///
    /// # Safety
    /// Must have exclusive ownership of EEPROM memory, WAITCNT's EEPROM wait control setting, and
    /// DMA3. Any DMA channels of higher priority should be disabled.
    pub unsafe fn new() -> Self {
        Self { _private: () }
    }

    /// Returns a reader over the given range.
    pub fn reader<'a, 'b, Range>(&'a mut self, range: Range) -> Reader512B<'a>
    where
        Range: RangeBounds<RangedUsize<0, 511>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range, EEPROM_MEMORY);
        unsafe { Reader512B::new_unchecked(address, len) }
    }

    /// Returns a writer over the given range.
    pub fn writer<'a, 'b, Range>(&'a mut self, range: Range) -> Writer512B<'a>
    where
        Range: RangeBounds<RangedUsize<0, 511>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range, EEPROM_MEMORY);
        unsafe { Writer512B::new_unchecked(address, len) }
    }
}

/// An EEPROM device with 8KiB of storage.
#[derive(Debug)]
pub struct Eeprom8K {
    _private: (),
}

impl Eeprom8K {
    /// Creates an accessor to the EEPROM 8KiB backup memory.
    ///
    /// # Safety
    /// Must have exclusive ownership of EEPROM memory, WAITCNT's EEPROM wait control setting, and
    /// DMA3. Any DMA channels of higher priority should be disabled.
    pub unsafe fn new() -> Self {
        Self { _private: () }
    }

    /// Returns a reader over the given range.
    pub fn reader<'a, 'b, Range>(&'a mut self, range: Range) -> Reader8K<'a>
    where
        Range: RangeBounds<RangedUsize<0, 8191>>,
        'a: 'b,
    {
        let (address, len) = translate_range_to_buffer(range, EEPROM_MEMORY);
        unsafe { Reader8K::new_unchecked(address, len) }
    }

    /// Returns a writer over the given range.
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
        drop(writer);

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
        drop(writer);

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
        drop(writer);

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
        drop(writer);

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
        drop(writer);

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
        drop(writer);

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
