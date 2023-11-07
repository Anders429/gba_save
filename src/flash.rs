use crate::mmio::{Cycles, WAITCNT};
use core::{cmp::min, hint::black_box, marker::PhantomData, ptr, time::Duration};
use embedded_io::Read;

const FLASH_MEMORY: *mut u8 = 0x0e00_0000 as *mut u8;
const BANK_SWITCH: *mut Bank = 0x0e00_0000 as *mut Bank;
const COMMAND: *mut Command = 0x0e00_5555 as *mut Command;
const COMMAND_ENABLE: *mut u8 = 0x0e00_2aaa as *mut u8;

const ENABLE: u8 = 0x55;
const ERASED: u8 = 0xff;

const SIZE_64KB: usize = 0x10000;

#[derive(Debug)]
pub enum Error {
    UnknownDeviceID(u16),
    VerificationFailed,
    OutOfBounds(Size),
}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            Self::UnknownDeviceID(_) => embedded_io::ErrorKind::NotFound,
            Self::VerificationFailed => embedded_io::ErrorKind::TimedOut,
            Self::OutOfBounds(_) => embedded_io::ErrorKind::AddrNotAvailable,
        }
    }
}

#[derive(Debug)]
#[repr(u8)]
enum Command {
    EraseChip = 0x10,
    EraseSector = 0x30,
    Erase = 0x80,
    EnterIDMode = 0x90,
    Write = 0xa0,
    SwitchBank = 0xb0,
    LeaveIDMode = 0xf0,
    Enable = 0xaa,
}

#[derive(Debug)]
enum Bank {
    _0,
    _1,
}

#[derive(Clone, Copy, Debug)]
pub enum Size {
    _64KB,
    _128KB,
}

impl Size {
    fn check_bounds(self, position: usize, len: usize) -> Result<(), Error> {
        match self {
            Self::_64KB => {
                if position + len > SIZE_64KB {
                    Err(Error::OutOfBounds(self))
                } else {
                    Ok(())
                }
            }
            Self::_128KB => {
                if position + len > SIZE_64KB * 2 {
                    Err(Error::OutOfBounds(self))
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Different flash chip devices, by ID code.
#[derive(Debug)]
enum Device {
    /// Macronix 128K
    MX29L010,
    /// Sanyo
    LE26FV10N1TS,
    /// Panasonic
    MN63F805MNP,
    /// Macronix 64K
    MX29L512,
    /// Atmel
    AT29LV512,
    /// SST
    LE39FW512,
}

impl Device {
    fn size(&self) -> Size {
        match self {
            Self::MX29L010 | Self::LE26FV10N1TS => Size::_128KB,
            _ => Size::_64KB,
        }
    }
}

impl TryFrom<u16> for Device {
    type Error = Error;

    fn try_from(id: u16) -> Result<Self, Self::Error> {
        match id {
            0x09c2 => Ok(Device::MX29L010),
            0x1362 => Ok(Device::LE26FV10N1TS),
            0x1b32 => Ok(Device::MN63F805MNP),
            0x1cc2 => Ok(Device::MX29L512),
            0x3d1f => Ok(Device::AT29LV512),
            0xd4b4 => Ok(Device::LE39FW512),
            _ => Err(Error::UnknownDeviceID(id)),
        }
    }
}

#[derive(Debug)]
pub struct Reader<'a> {
    address: *mut u8,
    len: usize,
    lifetime: PhantomData<&'a ()>,
}

impl embedded_io::ErrorType for Reader<'_> {
    type Error = Error;
}

impl Read for Reader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let mut read_count = 0;
        loop {
            if read_count >= min(buf.len(), self.len) {
                self.address = unsafe { self.address.add(read_count) };
                self.len -= read_count;
                return Ok(read_count);
            }

            let address = unsafe { self.address.add(read_count) };
            // If we have hit the border between banks, we must switch before reading more.
            if ptr::eq(address, unsafe { FLASH_MEMORY.add(SIZE_64KB) }) {
                switch_bank(Bank::_1);
            }

            unsafe {
                *buf.get_unchecked_mut(read_count) = address.read_volatile();
            }
            read_count += 1;
        }
    }
}

#[derive(Debug)]
pub struct Flash {
    device: Device,
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
            return Err(Error::VerificationFailed);
        }

        i += 1;
    }
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

fn switch_bank(bank: Bank) {
    send_command(Command::SwitchBank);
    unsafe {
        BANK_SWITCH.write_volatile(bank);
    }
}

impl Flash {
    pub unsafe fn new() -> Result<Self, Error> {
        let mut waitstate_control = unsafe { WAITCNT.read_volatile() };
        let previous_cycles = waitstate_control.cycles();
        waitstate_control.set_cycles(Cycles::_8);
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

        send_command(Command::LeaveIDMode);
        wait(Duration::from_millis(20));
        // Sanyo 128K device needs to have `LeaveIDMode` command sent twice.
        if matches!(device, Device::LE26FV10N1TS) {
            send_command(Command::LeaveIDMode);
            wait(Duration::from_millis(20));
        }

        let mut waitstate_control = unsafe { WAITCNT.read_volatile() };
        waitstate_control.set_cycles(previous_cycles);
        unsafe { WAITCNT.write_volatile(waitstate_control) };

        Ok(Self { device })
    }

    pub fn size(&self) -> Size {
        self.device.size()
    }

    pub fn reset(&mut self) -> Result<(), Error> {
        send_command(Command::Erase);
        send_command(Command::EraseChip);

        // Verify.
        verify_byte(FLASH_MEMORY, ERASED, Duration::from_millis(20))
    }

    pub fn read<'a, 'b>(&'a mut self, position: usize, len: usize) -> Result<Reader<'b>, Error>
    where
        'a: 'b,
    {
        let size = self.device.size();
        size.check_bounds(position, len)?;

        // For 128KB devices, we need to make sure we are in the right bank.
        if matches!(size, Size::_128KB) {
            if position >= SIZE_64KB {
                switch_bank(Bank::_0);
            } else {
                switch_bank(Bank::_1);
            }
        }

        Ok(Reader {
            address: unsafe { FLASH_MEMORY.add(position) },
            len,

            lifetime: PhantomData,
        })
    }
}
