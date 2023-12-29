#![no_std]
#![no_main]

extern crate gba;

use core::ops::Range;
use deranged::RangedUsize;
use embedded_io::{Read, Write};
use gba_save::sram::Sram;
use log::{error, info};
use mgba_log::fatal;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    #[cfg(debug_assertions)]
    {
        error!("{}", info);
        fatal!("Halting due to panic. See logs for `PanicInfo`.");
    }
    loop {}
}

#[no_mangle]
pub fn __sync_synchronize() {}

#[no_mangle]
pub fn main() {
    mgba_log::init().expect("must be running in mGBA");

    let mut sram = unsafe { Sram::new() };
    const RANGE: Range<RangedUsize<0, 32767>> =
        unsafe { RangedUsize::new_unchecked(0)..RangedUsize::new_unchecked(4) };

    // Write to SRAM.
    let write = &[1, 2, 3, 4];
    sram.writer(RANGE).write(write).expect("failed to write");

    // Read from SRAM.
    let mut read = [0; 4];
    sram.reader(RANGE).read(&mut read).expect("failed to read");

    // Verify that the data is the same.
    if read == *write {
        info!("success");
    } else {
        info!("failed");
    }
}
