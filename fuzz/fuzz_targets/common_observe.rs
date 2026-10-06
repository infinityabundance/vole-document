#![no_main]

//! Fuzz the Phase-12.7 common-observation dispatch surface.
//!
//! Format detection and capability discovery run on **arbitrary** bytes before
//! any observation is admitted (plan §DEC-5). Invariants: detection never
//! panics; the capability set is internally consistent (`common_supported`
//! agrees with `common_representations`); an unknown/opaque format advertises
//! nothing; unsupported observations are a typed capability decline, never an
//! approximation.

use libfuzzer_sys::fuzz_target;
use vole_document::field::capabilities::{
    capabilities_for_format, common_representations, common_supported,
};
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::observe::{Representation, Selector};
use vole_document::limits::Limits;

fn common_selectors() -> Vec<Selector> {
    vec![
        Selector::Metadata,
        Selector::Text,
        Selector::Heading(0),
        Selector::Heading(3),
        Selector::Block(1),
        Selector::Table(0),
        Selector::Cell {
            table: 0,
            row: 6,
            col: 1,
        },
        Selector::Resource(0),
        Selector::Link(0),
        Selector::SearchMatch(String::new()),
        Selector::Document,
        Selector::Page(1),
    ]
}

const REPRESENTATIONS: [Representation; 9] = [
    Representation::Metadata,
    Representation::Text,
    Representation::Structure,
    Representation::Operators,
    Representation::EncodedBytes,
    Representation::DecodedBytes,
    Representation::ExactBytes,
    Representation::Preview,
    Representation::FullDocument,
];

fuzz_target!(init: {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    let fmt = detect_document_format(data, Limits::STRICT);
    let caps = capabilities_for_format(fmt);
    assert!(!caps.to_json().is_empty());

    // An opaque input cannot serve any common selector.
    if fmt == DocumentFormat::Opaque {
        for sel in common_selectors() {
            for rep in REPRESENTATIONS {
                assert!(!common_supported(fmt, &sel, rep));
            }
        }
        return;
    }

    // Capability self-consistency: the predicate must exactly reflect the
    // advertised representations for a common selector.
    for sel in common_selectors() {
        let advertised = common_representations(fmt, &sel);
        for rep in REPRESENTATIONS {
            let supported = common_supported(fmt, &sel, rep);
            let in_list = advertised.is_some_and(|r| r.contains(&rep.name()));
            assert_eq!(
                supported,
                in_list,
                "common_supported disagrees with common_representations for {sel:?}/{rep:?}"
            );
        }
    }
});
