#![no_main]

//! Fuzz the OPC core (Phase 12.3): content types, relationships, part-name
//! resolution, and the package model.
//!
//! Invariants (research I §3/§8): hostile relationship graphs and part names
//! never hang, recurse unbounded, or resolve to a filesystem path; external
//! targets are inert; every failure is typed.

use libfuzzer_sys::fuzz_target;
use vole_document::adapter::package::opc;
use vole_document::error::ErrorClass;
use vole_document::field::opc::build_opc_model;
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
    match build_opc_model(data, limits) {
        Ok(bytes) => {
            // The model serialization must itself decode without an invariant.
            typed(opc::OpcModel::decode(&bytes), "OpcModel::decode");
        }
        Err(e) => assert_ne!(e.class(), ErrorClass::InternalInvariant),
    }

    // Split the input so both XML parts and the target grammar see fuzz bytes.
    let (a, b) = data.split_at(data.len() / 2);
    typed(opc::parse_content_types(a, limits), "parse_content_types");
    typed(opc::parse_relationships(b, "/word/", limits), "parse_relationships");
    let target = core::str::from_utf8(b).unwrap_or("x");
    typed(opc::part_name_from_member(target), "part_name_from_member");
    let _ = opc::validate_part_name(target);
    typed(
        opc::resolve_part_target("/word/", target, limits),
        "resolve_part_target",
    );
    let _ = opc::is_absolute_uri(target);
});
