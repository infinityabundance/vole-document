#![no_main]

//! Fuzz the bounded ZIP member decode path (Phase 12.2).
//!
//! For a scanned archive, every member's raw span is the exact leaf; decoding is
//! a *bounded* derived operation (`miniz_oxide` raw inflate, no second
//! inflater). Invariants (research I §8): output is capped by the declared size
//! and `Limits`; a stored member's CRC is verified with CRC-32/ISO-HDLC; every
//! failure is typed (never `InternalInvariant`), and no decode touches the
//! network, a path, or an unbounded allocation.

use libfuzzer_sys::fuzz_target;
use vole_document::adapter::package::{scan, verify_stored_crc};
use vole_document::error::ErrorClass;
use vole_document::field::derive::inflate_raw_deflate;
use vole_document::limits::Limits;

/// Bound the number of members decoded per input so the target stays fast.
const MAX_MEMBERS: usize = 16;

fuzz_target!(init: {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    let limits = Limits::STRICT;
    let physical = match scan(data, limits) {
        Ok(p) => p,
        Err(e) => {
            assert_ne!(e.class(), ErrorClass::InternalInvariant);
            return;
        }
    };
    let _ = physical.validate(data.len() as u64);
    for member in physical.members.iter().take(MAX_MEMBERS) {
        let raw = match physical.member_raw(member, data) {
            Ok(r) => r,
            Err(e) => {
                assert_ne!(e.class(), ErrorClass::InternalInvariant);
                continue;
            }
        };
        let outcome = match member.method {
            0 => verify_stored_crc(member, raw),
            8 => inflate_raw_deflate(raw, member.uncompressed_size, limits).map(|_| ()),
            // Every other method is a typed decode decline; exact bytes untouched.
            _ => continue,
        };
        if let Err(e) = outcome {
            assert_ne!(
                e.class(),
                ErrorClass::InternalInvariant,
                "member decode reported an internal invariant"
            );
        }
    }
});
