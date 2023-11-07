use core::mem::transmute;

pub(crate) const WAITCNT: *mut WaitstateControl = 0x0400_0204 as *mut WaitstateControl;

#[derive(Debug)]
#[repr(u8)]
pub(crate) enum Cycles {
    _4 = 0,
    _3 = 1,
    _2 = 2,
    _8 = 3,
}

#[derive(Debug)]
pub(crate) struct WaitstateControl(u16);

impl WaitstateControl {
    pub(crate) fn cycles(&self) -> Cycles {
        unsafe { transmute((self.0 & 0b0000_0000_0000_0011) as u8) }
    }

    pub(crate) fn set_cycles(&mut self, cycles: Cycles) {
        self.0 ^= 0b1111_1111_1111_1100;
        self.0 &= cycles as u16;
    }
}
