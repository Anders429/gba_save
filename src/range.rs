use core::ops::{Bound, RangeBounds};
use deranged::RangedUsize;

pub(crate) fn translate_range_to_buffer<const MAX: usize, Range>(
    range: Range,
    address_offset: *mut u8,
) -> (*mut u8, usize)
where
    Range: RangeBounds<RangedUsize<0, MAX>>,
{
    let offset = match range.start_bound() {
        Bound::Included(start) => start.get(),
        Bound::Excluded(start) => start.get() + 1,
        Bound::Unbounded => 0,
    };
    let address = unsafe { address_offset.add(offset) };
    let len = match range.end_bound() {
        Bound::Included(end) => end.get() + 1,
        Bound::Excluded(end) => end.get(),
        Bound::Unbounded => MAX + 1,
    } - offset;
    (address, len)
}

#[cfg(test)]
mod tests {
    use super::translate_range_to_buffer;
    use deranged::RangedUsize;
    use gba_test::test;
    use more_ranges::{
        RangeFromExclusive, RangeFromExclusiveToExclusive, RangeFromExclusiveToInclusive,
    };

    // SRAM memory location, which is fine to use for the tests.
    const MEMORY: *mut u8 = 0x0e00_0000 as *mut u8;

    #[test]
    fn unbounded_unbounded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(.., MEMORY),
            (MEMORY, 32768)
        );
    }

    #[test]
    fn unbounded_included() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(..=RangedUsize::new_static::<42>(), MEMORY),
            (MEMORY, 43)
        );
    }

    #[test]
    fn unbounded_excluded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(..RangedUsize::new_static::<42>(), MEMORY),
            (MEMORY, 42)
        );
    }

    #[test]
    fn included_unbounded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(RangedUsize::new_static::<42>().., MEMORY),
            (unsafe { MEMORY.add(42) }, 32726)
        );
    }

    #[test]
    fn included_included() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(
                RangedUsize::new_static::<42>()..=RangedUsize::new_static::<100>(),
                MEMORY
            ),
            (unsafe { MEMORY.add(42) }, 59)
        );
    }

    #[test]
    fn included_excluded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(
                RangedUsize::new_static::<42>()..RangedUsize::new_static::<100>(),
                MEMORY
            ),
            (unsafe { MEMORY.add(42) }, 58)
        );
    }

    #[test]
    fn excluded_unbounded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(
                RangeFromExclusive {
                    start: RangedUsize::new_static::<42>()
                },
                MEMORY
            ),
            (unsafe { MEMORY.add(43) }, 32725)
        );
    }

    #[test]
    fn excluded_included() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(
                RangeFromExclusiveToInclusive {
                    start: RangedUsize::new_static::<42>(),
                    end: RangedUsize::new_static::<100>()
                },
                MEMORY
            ),
            (unsafe { MEMORY.add(43) }, 58)
        );
    }

    #[test]
    fn excluded_excluded() {
        assert_eq!(
            translate_range_to_buffer::<32767, _>(
                RangeFromExclusiveToExclusive {
                    start: RangedUsize::new_static::<42>(),
                    end: RangedUsize::new_static::<100>()
                },
                MEMORY
            ),
            (unsafe { MEMORY.add(43) }, 57)
        );
    }
}
