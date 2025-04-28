#![no_std]
#![cfg_attr(test, no_main)]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(gba_test::runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_harness")]

pub mod eeprom;
pub mod flash;
pub mod sram;

mod mmio;
mod range;

#[cfg(test)]
#[no_mangle]
pub fn main() {
    let _ = mgba_log::init();
    test_harness()
}
