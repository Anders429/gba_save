use crate::mmio::{Cycles, WAITCNT};
use core::{hint::black_box, time::Duration};

const FLASH_MEMORY: *mut u8 = 0x0e00_0000 as *mut u8;
const COMMAND: *mut Command = 0x0e00_5555 as *mut Command;
const COMMAND_ENABLE: *mut u8 = 0x0e00_2aaa as *mut u8;

const ENABLE: u8 = 0x55;

pub enum Error {
    UnknownDeviceId(u16),
}

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

/// Different flash chip devices, by ID code.
enum Device {
    /// Macronix 128K
    MX29L010 = 0x09c2,
    /// Sanyo
    LE26FV10N1TS = 0x1362,
    /// Panasonic
    MN63F805MNP = 0x1b32,
    /// Macronix 64K
    MX29L512 = 0x1cc2,
    /// Atmel
    AT29LV512 = 0x3d1f,
    /// SST
    LE39FW512 = 0xd4b4,
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
            _ => Err(Error::UnknownDeviceId(id)),
        }
    }
}

enum Size {
    KB64,
    KB128,
}

pub struct Flash {
    device: Device,
}

fn wait(amount: Duration) {
    for _ in 0..amount.as_millis() * 1000 {
        black_box(());
    }
}

fn begin_send_command() {
    unsafe {
        *COMMAND = Command::Enable;
        *COMMAND_ENABLE = ENABLE;
    }
}

fn send_command(command: Command) {
    begin_send_command();
    unsafe {
        *COMMAND = command;
    }
}

impl Flash {
    pub fn new() -> Result<Self, Error> {
        let mut waitstate_control = unsafe { WAITCNT.read_volatile() };
        let previous_cycles = waitstate_control.cycles();
        waitstate_control.set_cycles(Cycles::_8);
        unsafe { WAITCNT.write_volatile(waitstate_control) };

        send_command(Command::EnterIDMode);
        wait(Duration::from_millis(20));

        // Read u16 from memory.
        let device = u16::from_ne_bytes(unsafe {
            [
                FLASH_MEMORY.add(1).read_volatile(),
                FLASH_MEMORY.read_volatile(),
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
}
