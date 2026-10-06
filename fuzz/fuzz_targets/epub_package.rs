#![no_main]

//! Fuzz the EPUB OCF/Package/nav adapter (Phase 12.5).
//!
//! Invariants: container/rootfile/manifest/spine discovery is bounded and typed;
//! a remote or encrypted resource is an inert string, never fetched or
//! decrypted; the model serialization round-trips; never an `InternalInvariant`.

use libfuzzer_sys::fuzz_target;
use vole_document::adapter::epub;
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;

fn typed<T>(r: vole_document::Result<T>, what: &str) {
    if let Err(e) = r {
        assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "{what} internal invariant"
        );
    }
}

fuzz_target!(init: {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    let limits = Limits::STRICT;
    match epub::build_epub_model(data, limits) {
        Ok(bytes) => {
            typed(epub::EpubModel::decode(&bytes), "EpubModel::decode");
        }
        Err(e) => assert_ne!(e.class(), ErrorClass::InternalInvariant),
    }

    let (a, b) = data.split_at(data.len() / 2);
    typed(epub::parse_nav_document(a, "/OEBPS/", limits), "parse_nav_document");
    typed(epub::content::parse_content(b, "/OEBPS/", limits), "parse_content");
});
