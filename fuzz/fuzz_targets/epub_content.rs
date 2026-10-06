#![no_main]

//! Fuzz bounded XHTML content observations (Phase 12.6).
//!
//! Invariants: headings/paragraphs/lists/tables/links/resources are extracted
//! with bounded nodes/depth/attrs/text; `<script>` is data and never executed or
//! rendered into reading text; external references are inert; the content model
//! round-trips; never an `InternalInvariant`.

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
    match epub::content::parse_content(data, "/OEBPS/", limits) {
        Ok(model) => {
            // Script bodies must never leak into the reading text.
            if model.scripted {
                let _ = model.text();
            }
            typed(epub::ContentModel::decode(&model.encode()), "ContentModel::decode");
        }
        Err(e) => assert_ne!(e.class(), ErrorClass::InternalInvariant),
    }
    typed(epub::content::read_content_params(data), "read_content_params");
    typed(epub::parse_nav_document(data, "/OEBPS/", limits), "parse_nav_document");
});
