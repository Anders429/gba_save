# Changelog

## Unreleased
### Added
- `flash::device::UnknownDeviceId` now implements `core::error::Error`.
### Fixed
- Disable flush check on drop for EEPROM and 64KiB Atmel Flash writers when failures occur due to to a problem with the device.

## 0.3.0 - 2025-09-26
### Changed
- Now requiring the 2024 edition of Rust.
### Fixed
- Unaligned EEPROM writes now correctly preserve data that is already written.
- Flushing EEPROM when the internal buffer location is not aligned now correctly updates the writer's state.
- Flushing EEPROM now sends a correct read request when loading unaligned data.
- EEPROM Reading across alignment boundaries now advances to the next address in all cases.

## 0.2.0 - 2025-09-24 [YANKED]
### Added
- All error types now implement `From<embedded_io::ReadExactError>`, converting `ReadExactError::UnexpectedEof` into `Error::EndOfWriter` for each of the various error types.

## 0.1.1 - 2025-08-30
### Changed
- EEPROM is now accessed at 0x0DFFFF00 instead of 0x0D000000, allowing better operation with 32MiB ROM sizes.
- Upgraded `deranged` version to `0.5.2`.
### Fixed
- Corrected some examples in the documentation.

## 0.1.0 - 2025-05-04
### Added
- `sram` module allowing reading and writing from SRAM.
- `eeprom` module allowing reading and writing from EEPROM, supporting both 512B and 8KiB chips.
- `flash` module allowing reading and writing from Flash, supporting both 64KiB and 128KiB, including special handling for Atmel chips.
