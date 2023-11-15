pub(crate) const WAITCNT: *mut WaitstateControl = 0x0400_0204 as *mut WaitstateControl;
/// Interrupt Master Enable.
///
/// This register allows enabling and disabling interrupts.
pub(crate) const IME: *mut bool = 0x0400_0208 as *mut bool;

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
    pub(crate) fn set_backup_waitstate(&mut self, cycles: Cycles) {
        self.0 ^= 0b1111_1111_1111_1100;
        self.0 &= cycles as u16;
    }
}
