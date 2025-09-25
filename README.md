# gba_save

[![GitHub Workflow Status](https://img.shields.io/github/check-runs/Anders429/gba_save/master?label=tests)](https://github.com/Anders429/gba_save/actions?query=branch%3Amaster)
[![crates.io](https://img.shields.io/crates/v/gba_save)](https://crates.io/crates/gba_save)
[![docs.rs](https://docs.rs/gba_save/badge.svg)](https://docs.rs/gba_save)
[![License](https://img.shields.io/crates/l/gba_save)](#license)

Tools for interacting with backup media on Game Boy Advance cartridges.

The Game Boy Advance has three forms of backup media for saving data: SRAM, EEPROM, and Flash. This library provides tools for interacting with all three different types.

## Example Usage
To write and read save data using SRAM, use something like the following:

``` rust
use deranged::RangedUsize;
use embedded_io::{Read, Write};
use gba_save::sram::Sram;

let mut sram = unsafe {Sram::new()};
let mut writer = sram.writer(RangedUsize::new_static<0>()..RangedUsize::new_static<15>());

// Write some data.
//
// Note that you'll usually want to handle the error here.
writer.write(b"hello, world!").expect("could not write to SRAM");

// Write some more data.
writer.write(b"123").expect("could not write to SRAM");

// Read the data back.
let mut reader = sram.reader(RangedUsize::new_static<0>()..RangedUsize::new_static<15>());
let mut buffer = [0; 16];
assert_eq!(reader.read(&mut buf), 16);
// Both things that were written will be read back.
assert_eq!(buffer, b"hello, world!123");
```

See the documentation for more details and examples for interacting with SRAM and the other backup media types.

## Optional Features
- **`serde`**: Enable serializing and deserializing the variuos error types using the [`serde`](https://crates.io/crates/serde) library.
- **`log`**: Enable log messages using the [`log`](https://crates.io/crates/log) library.
Helpful for development. This is best used when paired with a logger like [`mgba_log`](https://crates.io/crates/mgba_log) or
[`nocash_gba_log`](https://crates.io/crates/nocash_gba_log).

## License
This project is licensed under either of

* Apache License, Version 2.0
([LICENSE-APACHE](https://github.com/Anders429/gba_save/blob/HEAD/LICENSE-APACHE) or
http://www.apache.org/licenses/LICENSE-2.0)
* MIT license
([LICENSE-MIT](https://github.com/Anders429/gba_save/blob/HEAD/LICENSE-MIT) or
http://opensource.org/licenses/MIT)

at your option.

### Contribution
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
