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
        self.0 &= 0b1111_1111_1111_1100;
        self.0 |= cycles as u16;
    }
}

#[cfg(test)]
mod tests {
    use super::{Cycles, WaitstateControl};
    use gba_test::test;

    #[test]
    fn set_backup_waitstate_4() {
        let mut waitstate = WaitstateControl(0);
        waitstate.set_backup_waitstate(Cycles::_4);

        assert_eq!(waitstate.0, 0);
    }

    #[test]
    fn set_backup_waitstate_3() {
        let mut waitstate = WaitstateControl(0);
        waitstate.set_backup_waitstate(Cycles::_3);

        assert_eq!(waitstate.0, 1);
    }

    #[test]
    fn set_backup_waitstate_2() {
        let mut waitstate = WaitstateControl(0);
        waitstate.set_backup_waitstate(Cycles::_2);

        assert_eq!(waitstate.0, 2);
    }

    #[test]
    fn set_backup_waitstate_8() {
        let mut waitstate = WaitstateControl(0);
        waitstate.set_backup_waitstate(Cycles::_8);

        assert_eq!(waitstate.0, 3);
    }

    #[test]
    fn set_backup_waitstate_with_preexisting_value() {
        let mut waitstate = WaitstateControl(3);
        waitstate.set_backup_waitstate(Cycles::_4);

        assert_eq!(waitstate.0, 0);
    }
}
