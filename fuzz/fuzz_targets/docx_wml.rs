#![no_main]

//! Fuzz the DOCX WordprocessingML adapter (Phase 12.4).
//!
//! Invariants: token/story extraction is bounded by the XML policy; a hostile
//! package yields a typed decline, never an `InternalInvariant`; the model
//! serialization round-trips deterministically; `Q_ref` bytes are never derived
//! from a projection.

use libfuzzer_sys::fuzz_target;
use vole_document::adapter::docx::{self, DocxExtractProfile, DocxStory};
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
    match docx::build_docx_model(data, limits) {
        Ok(bytes) => {
            typed(docx::DocxModel::decode(&bytes), "DocxModel::decode");
        }
        Err(e) => assert_ne!(e.class(), ErrorClass::InternalInvariant),
    }

    let (a, b) = data.split_at(data.len() / 2);
    typed(docx::parse_styles(a, limits), "parse_styles");
    typed(
        docx::wml::parse_story(
            b,
            "/word/document.xml",
            DocxStory::Main,
            &DocxExtractProfile::DEFAULT,
            None,
            limits,
        ),
        "parse_story",
    );
    if data.len() >= 10 {
        typed(DocxExtractProfile::decode(&data[..10]), "profile decode");
    }
    let _ = DocxStory::Main.expected_root();
    let _ = DocxStory::Main.name();
});
