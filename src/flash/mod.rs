mod device;
mod error;
mod reader;
mod writer;

pub use device::UnknownDeviceID;
pub use error::Error;
pub use reader::{Reader128K, Reader64K};
pub use writer::{Writer128K, Writer64K, Writer64KAtmel};

use crate::mmio::{Cycles, WAITCNT};
use core::{
    hint::black_box,
    ops,
    ops::{Bound, RangeBounds},
    time::Duration,
};
use deranged::{RangedU8, RangedUsize};
use device::Device;

const FLASH_MEMORY: *mut u8 = 0x0e00_0000 as *mut u8;
const BANK_SWITCH: *mut Bank = 0x0e00_0000 as *mut Bank;
const COMMAND: *mut Command = 0x0e00_5555 as *mut Command;
const COMMAND_ENABLE: *mut u8 = 0x0e00_2aaa as *mut u8;
const SECTOR_COMMAND: *mut Command = 0x0e00_0000 as *mut Command;
const ENABLE: u8 = 0x55;
const ERASED: u8 = 0xff;
const SIZE_64KB: usize = 0x10000;

#[derive(Debug)]
#[repr(u8)]
enum Command {
    EraseChip = 0x10,
    EraseSector = 0x30,
    Erase = 0x80,
    EnterIDMode = 0x90,
    Write = 0xa0,
    SwitchBank = 0xb0,
    TerminateMode = 0xf0,
    Enable = 0xaa,
}

fn begin_send_command() {
    unsafe {
        COMMAND.write_volatile(Command::Enable);
        COMMAND_ENABLE.write_volatile(ENABLE);
    }
}

fn send_command(command: Command) {
    begin_send_command();
    unsafe {
        COMMAND.write_volatile(command);
    }
}

#[derive(Clone, Copy, Debug)]
enum Bank {
    _0,
    _1,
}

fn switch_bank(bank: Bank) {
    send_command(Command::SwitchBank);
    unsafe {
        BANK_SWITCH.write_volatile(bank);
    }
}

fn wait(amount: Duration) {
    for _ in 0..amount.as_millis() * 1000 {
        black_box(());
    }
}

fn verify_byte(address: *const u8, byte: u8, timeout: Duration) -> Result<(), Error> {
    let mut i = 0;
    loop {
        if unsafe { address.read_volatile() } == byte {
            return Ok(());
        }
        if i >= timeout.as_millis() * 1000 {
            return Err(Error::OperationTimedOut);
        }

        i += 1;
    }
}

fn verify_bytes(address: *const u8, bytes: &[u8], timeout: Duration) -> Result<(), Error> {
    let mut i = 0;
    loop {
        let mut verified = true;
        for (i, &byte) in bytes.iter().enumerate() {
            if unsafe { address.add(i).read_volatile() } != byte {
                verified = false;
                break;
            }
        }
        if verified {
            return Ok(());
        }
        if i >= timeout.as_millis() * 1000 {
            return Err(Error::OperationTimedOut);
        }

        i += 1;
    }
}

fn erase_sector(sector: u8) -> Result<(), Error> {
    // Generic erase command.
    send_command(Command::Erase);

    // Specific erase command for sector.
    begin_send_command();
    let sector_command = unsafe { SECTOR_COMMAND.add(sector as usize * 0x1000) };
    unsafe {
        sector_command.write_volatile(Command::EraseSector);
    }

    verify_byte(
        sector_command as *const u8,
        ERASED,
        Duration::from_millis(20),
    )
}

fn translate_range_to_buffer<const MAX: usize, Range>(range: Range) -> (*mut u8, usize)
where
    Range: RangeBounds<RangedUsize<0, MAX>>,
{
    let offset = match range.start_bound() {
        Bound::Included(start) => start.get(),
        Bound::Excluded(start) => start.get().saturating_sub(1),
        Bound::Unbounded => 0,
    };
    let address = unsafe { FLASH_MEMORY.add(offset) };
    let len = match range.end_bound() {
        Bound::Included(end) => end.get() + 1,
        Bound::Excluded(end) => end.get(),
        Bound::Unbounded => MAX + 1,
    } - offset;
    (address, len)
}

fn translate_range_to_sectors<const MAX: u8, Range>(range: Range) -> ops::Range<u8>
where
    Range: RangeBounds<RangedU8<0, MAX>>,
{
    #[allow(unused_parens)] // Doesn't compile without the parenthesis.
    (match range.start_bound() {
        Bound::Included(start) => start.get(),
        Bound::Excluded(start) => start.get().saturating_sub(1),
        Bound::Unbounded => 0,
    }..match range.end_bound() {
        Bound::Included(end) => end.get() + 1,
        Bound::Excluded(end) => end.get(),
        Bound::Unbounded => MAX + 1,
    })
}

#[derive(Debug)]
pub struct Flash64K;

impl Flash64K {
    pub fn reader<'a, 'b, Range>(&'a mut self, range: Range) -> Reader64K<'b>
    where
        'a: 'b,
        Range: RangeBounds<RangedUsize<0, 65535>>,
    {
        let (address, len) = translate_range_to_buffer(range);
        unsafe { Reader64K::new_unchecked(address, len) }
    }

    pub fn erase_sectors<Range>(&mut self, sectors: Range) -> Result<(), Error>
    where
        Range: RangeBounds<RangedU8<0, 15>>,
    {
        for sector in translate_range_to_sectors(sectors) {
            erase_sector(sector)?;
        }
        Ok(())
    }

    pub fn writer<'a, 'b, Range>(&'a mut self, range: Range) -> Writer64K<'b>
    where
        'a: 'b,
        Range: RangeBounds<RangedUsize<0, 65535>>,
    {
        let (address, len) = translate_range_to_buffer(range);
        unsafe { Writer64K::new_unchecked(address, len) }
    }
}

#[derive(Debug)]
pub struct Flash64KAtmel;

impl Flash64KAtmel {
    pub fn reader<'a, 'b, Range>(&'a mut self, range: Range) -> Reader64K<'b>
    where
        'a: 'b,
        Range: RangeBounds<RangedUsize<0, 65535>>,
    {
        let (address, len) = translate_range_to_buffer(range);
        unsafe { Reader64K::new_unchecked(address, len) }
    }

    pub fn writer<'a, 'b, Range>(&'a mut self, range: Range) -> Writer64KAtmel<'b>
    where
        'a: 'b,
        Range: RangeBounds<RangedUsize<0, 65535>>,
    {
        let (address, len) = translate_range_to_buffer(range);
        unsafe { Writer64KAtmel::new_unchecked(address, len) }
    }
}

#[derive(Debug)]
pub struct Flash128K;

impl Flash128K {
    pub fn reader<'a, 'b, Range>(&'a mut self, range: Range) -> Reader128K<'b>
    where
        'a: 'b,
        Range: RangeBounds<RangedUsize<0, 131071>>,
    {
        let (address, len) = translate_range_to_buffer(range);
        unsafe { Reader128K::new_unchecked(address, len) }
    }

    pub fn erase_sectors<Range>(&mut self, sectors: Range) -> Result<(), Error>
    where
        Range: RangeBounds<RangedU8<0, 31>>,
    {
        let sectors_range = translate_range_to_sectors(sectors);
        let mut bank = if sectors_range.start < 16 {
            Bank::_0
        } else {
            Bank::_1
        };
        switch_bank(bank);
        for mut sector in sectors_range {
            if matches!(bank, Bank::_0) {
                if sector >= 16 {
                    bank = Bank::_1;
                    switch_bank(bank);
                }
            }
            if matches!(bank, Bank::_1) {
                sector %= 16;
            }
            erase_sector(sector)?;
        }
        Ok(())
    }

    pub fn writer<'a, 'b, Range>(&'a mut self, range: Range) -> Writer128K<'b>
    where
        'a: 'b,
        Range: RangeBounds<RangedUsize<0, 131071>>,
    {
        let (address, len) = translate_range_to_buffer(range);
        unsafe { Writer128K::new_unchecked(address, len) }
    }
}

#[derive(Debug)]
pub enum Flash {
    Flash64K(Flash64K),
    Flash64KAtmel(Flash64KAtmel),
    Flash128K(Flash128K),
}

impl Flash {
    /// # Safety
    /// Must have exclusive ownership of both flash RAM memory and WAITCNT's SRAM wait control
    /// setting for the duration of its lifetime.
    pub unsafe fn new() -> Result<Self, UnknownDeviceID> {
        let mut waitstate_control = unsafe { WAITCNT.read_volatile() };
        waitstate_control.set_backup_waitstate(Cycles::_8);
        unsafe { WAITCNT.write_volatile(waitstate_control) };

        send_command(Command::EnterIDMode);
        wait(Duration::from_millis(20));

        // Read u16 from memory.
        let device = u16::from_ne_bytes(unsafe {
            [
                FLASH_MEMORY.read_volatile(),
                FLASH_MEMORY.add(1).read_volatile(),
            ]
        })
        .try_into()?;

        send_command(Command::TerminateMode);
        wait(Duration::from_millis(20));
        // Sanyo 128K device needs to have `TerminateMode` command sent twice.
        if matches!(device, Device::LE26FV10N1TS) {
            send_command(Command::TerminateMode);
            wait(Duration::from_millis(20));
        }

        match device {
            Device::AT29LV512 => Ok(Self::Flash64KAtmel(Flash64KAtmel)),
            Device::MX29L010 | Device::LE26FV10N1TS => Ok(Self::Flash128K(Flash128K)),
            _ => Ok(Self::Flash64K(Flash64K)),
        }
    }

    pub fn reset(&mut self) -> Result<(), Error> {
        send_command(Command::Erase);
        send_command(Command::EraseChip);

        // Verify.
        verify_byte(FLASH_MEMORY, ERASED, Duration::from_millis(20))
    }
}
