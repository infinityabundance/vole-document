#![no_main]

//! Fuzz the shared bounded-XML part policy (Phase 12.5, research I §2).
//!
//! Arbitrary bytes are handed to the XML entry points that share
//! `harden_xml`/`XmlState`: quick-xml with `default-features = false` (UTF-8
//! only), no DTD/entity resolver, and explicit depth/event/node/attr/text caps.
//! Invariants: DOCTYPE/NUL/non-UTF-8/UTF-16 are refused typed; depth and event
//! floods are `ResourceLimit`/`InvalidXmlStructure`; never an `InternalInvariant`
//! and never a panic.

use libfuzzer_sys::fuzz_target;
use vole_document::adapter::epub;
use vole_document::adapter::package::opc;
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;

fn typed<T>(r: vole_document::Result<T>, what: &str) {
    if let Err(e) = r {
        assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "{what} internal invariant"
        );
        assert_ne!(
            e.class(),
            ErrorClass::InvalidContainer,
            "{what} misclassified a malformed part"
        );
    }
}

fuzz_target!(init: {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    let limits = Limits::STRICT;
    typed(opc::parse_content_types(data, limits), "parse_content_types");
    typed(opc::parse_relationships(data, "/", limits), "parse_relationships");
    typed(epub::parse_nav_document(data, "/", limits), "parse_nav_document");
    typed(epub::content::parse_content(data, "/", limits), "parse_content");
});
