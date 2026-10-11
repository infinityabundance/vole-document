//! Phase 21.24 court: the Jupyter notebook (`.ipynb`, nbformat) adapter.
//!
//! Gated on `field` + `notebook` (which implies `json`), so a build without the
//! feature compiles an empty target.
//!
//! A notebook's physical bytes are JSON, so its representation-preserving parser is the
//! **shared** JSON adapter (never a second JSON parser). The distinct claim is a
//! **bounded semantic sub-detection** run before the generic JSON detector. The court
//! checks:
//!
//! 1. **Byte-based detection and the boundaries**: an nbformat document → `Notebook`;
//!    a plain JSON document, a JSON document that merely has a `cells` key, a `cells`
//!    array of non-objects, and a non-integer `nbformat` all stay `Json`; prose stays
//!    `Opaque`.
//! 2. **Representation preservation**: the exact `cell_type`/`output_type` strings; the
//!    exact `source` representation (a string vs an array of lines, never re-joined);
//!    the exact `execution_count`; cell/output order; `metadata`/`attachments`.
//! 3. **Selectors**: native `notebook-nbformat`/`notebook-cell`/`notebook-cell-type`/
//!    `notebook-cell-source`/`notebook-cell-output`/`notebook-find` plus common
//!    `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: malformed declines typed; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "notebook"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_NOTEBOOK_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-notebook-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:notebook-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

struct Fixture {
    root: PathBuf,
    store: FieldStore,
    report: IngestReport,
    source: Vec<u8>,
}

impl Fixture {
    fn new(label: &str, source: &[u8]) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let descriptor = opaque_descriptor(source);
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        Fixture {
            root,
            store,
            report,
            source: source.to_vec(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn observe_ok(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> FieldAnswer {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT).unwrap().0
}

fn answer_text(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Text(t) => t.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn answer_json(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Json(j) => j.clone(),
        other => panic!("expected json, got {other:?}"),
    }
}

fn answer_bytes(answer: &FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

/// A full nbformat 4 notebook: a markdown cell (line-array source + attachments), a
/// code cell (string source + one output of each type), and a raw cell.
const NB_MAIN: &[u8] = br##"{
  "cells": [
    {"cell_type": "markdown", "metadata": {"id": "m1"},
     "source": ["# Title\n", "Some *text*.\n"],
     "attachments": {"img.png": {"image/png": "aGk="}}},
    {"cell_type": "code", "execution_count": 7, "metadata": {"collapsed": false},
     "source": "print(1 + 1)\n",
     "outputs": [
       {"output_type": "stream", "name": "stdout", "text": ["2\n"]},
       {"output_type": "execute_result", "execution_count": 7,
        "data": {"text/plain": "2"}, "metadata": {}},
       {"output_type": "display_data", "data": {"image/png": "aGk="}, "metadata": {}},
       {"output_type": "error", "ename": "ValueError", "evalue": "boom",
        "traceback": ["Traceback (most recent call last):", "ValueError: boom"]}
     ]},
    {"cell_type": "raw", "metadata": {}, "source": "raw text\n"}
  ],
  "metadata": {"kernelspec": {"display_name": "Python 3", "name": "python3"},
               "language_info": {"name": "python", "version": "3.11"}},
  "nbformat": 4,
  "nbformat_minor": 5
}"##;

/// An nbformat 3 notebook (older schema: no `nbformat_minor`, an `input` field, and a
/// `worksheets` array) — still nbformat-shaped.
const NB_V3: &[u8] = br#"{"metadata": {}, "nbformat": 3, "nbformat_minor": 0,
  "worksheets": [{"cells": []}],
  "cells": [{"cell_type": "code", "input": "1+1", "outputs": [], "language": "python"}]}"#;

/// A minimal nbformat 4 notebook (empty cell list).
const NB_MIN: &[u8] = br#"{"nbformat":4,"cells":[]}"#;

/// Plain JSON: not a notebook (stays `Json`).
const PLAIN_JSON: &[u8] = br#"{"a":1,"b":[2,3]}"#;
/// A JSON document that merely has a `cells` key but no `nbformat` (stays `Json`).
const CELLS_ONLY: &[u8] = br#"{"cells":[{"cell_type":"code"}]}"#;
/// A JSON document whose `cells` are not objects (stays `Json`).
const CELLS_NOT_OBJECTS: &[u8] = br#"{"nbformat":4,"cells":[1,2,3]}"#;
/// A JSON document whose `nbformat` is not an integer (stays `Json`).
const BAD_NBFORMAT: &[u8] = br#"{"nbformat":"4","cells":[]}"#;
/// A JSON document whose `nbformat` is a float literal (stays `Json`).
const FLOAT_NBFORMAT: &[u8] = br#"{"nbformat":4.0,"cells":[]}"#;
/// Plain prose: never a notebook (stays `Opaque`).
const PROSE: &[u8] =
    b"The quick brown fox jumps over the lazy dog.\nPlain prose, not a notebook.\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for (bytes, want) in [
        (NB_MAIN, DocumentFormat::Notebook),
        (NB_V3, DocumentFormat::Notebook),
        (NB_MIN, DocumentFormat::Notebook),
        // The documented boundaries: plain JSON, a bare `cells` key, a non-object
        // `cells` array, and a non-integer `nbformat` all stay `Json`; prose `Opaque`.
        (PLAIN_JSON, DocumentFormat::Json),
        (CELLS_ONLY, DocumentFormat::Json),
        (CELLS_NOT_OBJECTS, DocumentFormat::Json),
        (BAD_NBFORMAT, DocumentFormat::Json),
        (FLOAT_NBFORMAT, DocumentFormat::Json),
        (PROSE, DocumentFormat::Opaque),
    ] {
        assert_eq!(
            detect_document_format(bytes, Limits::DEFAULT),
            want,
            "detection boundary: {:?}",
            String::from_utf8_lossy(bytes)
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Notebook);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for want in ["metadata", "text", "search-match"] {
        assert!(names.contains(&want), "missing common selector {want}");
    }
    let native = &caps.native_selectors;
    for want in [
        "notebook-nbformat",
        "notebook-cell",
        "notebook-cell-type",
        "notebook-cell-source",
        "notebook-cell-output",
        "notebook-find",
    ] {
        assert!(native.contains(&want), "missing native selector {want}");
    }
}

#[test]
fn cell_types_source_forms_and_order_are_preserved() {
    use vole_document::adapter::notebook::{
        self, C_CODE, C_MARKDOWN, C_RAW, SRC_LINES, SRC_STRING,
    };

    let m = notebook::parse(NB_MAIN, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.nbformat, 4);
    assert_eq!(m.nbformat_minor, 5);
    assert_eq!(m.cell_count(), 3);
    // Exact cell_type strings and their classes, in document order.
    assert_eq!(m.cells[0].class, C_MARKDOWN);
    assert_eq!(m.cells[1].class, C_CODE);
    assert_eq!(m.cells[2].class, C_RAW);
    assert_eq!(
        notebook::cell_type_of(&m, NB_MAIN, m.cells[1].node)
            .unwrap()
            .unwrap(),
        "code"
    );
    // The exact cell_type token is preserved.
    let ct = m.node(m.cells[1].cell_type_node).unwrap();
    assert_eq!(notebook::token_bytes(NB_MAIN, ct).unwrap(), b"\"code\"");
    // Source representation: markdown is an array of lines; code is a string.
    assert_eq!(m.cells[0].source_form, SRC_LINES);
    assert_eq!(m.cells[1].source_form, SRC_STRING);
    assert_eq!(m.cells[2].source_form, SRC_STRING);
    let lines = m.node(m.cells[0].source_node).unwrap();
    assert_eq!(lines.children.len(), 2);
    let s = m.node(m.cells[1].source_node).unwrap();
    assert_eq!(
        notebook::token_bytes(NB_MAIN, s).unwrap(),
        b"\"print(1 + 1)\\n\""
    );
    // The exact execution_count token is preserved.
    let ec = m.node(m.cells[1].execution_count_node).unwrap();
    assert_eq!(notebook::token_bytes(NB_MAIN, ec).unwrap(), b"7");
    // Attachments are preserved as member keys.
    let att = notebook::member_keys(&m, NB_MAIN, m.cells[0].node, "attachments")
        .unwrap()
        .unwrap();
    assert_eq!(att, vec!["img.png"]);
}

#[test]
fn outputs_of_each_type_are_preserved() {
    use vole_document::adapter::json::{K_ARRAY, K_STRING};
    use vole_document::adapter::notebook::{
        self, O_DISPLAY_DATA, O_ERROR, O_EXECUTE_RESULT, O_STREAM, output_class_of, output_type_of,
    };

    let m = notebook::parse(NB_MAIN, Limits::DEFAULT, true).unwrap();
    let code = &m.cells[1];
    assert_eq!(code.outputs.len(), 4);
    let types: Vec<String> = code
        .outputs
        .iter()
        .map(|&o| output_type_of(&m, NB_MAIN, o).unwrap().unwrap())
        .collect();
    assert_eq!(
        types,
        vec!["stream", "execute_result", "display_data", "error"]
    );
    assert_eq!(output_class_of(&types[0]), O_STREAM);
    assert_eq!(output_class_of(&types[1]), O_EXECUTE_RESULT);
    assert_eq!(output_class_of(&types[2]), O_DISPLAY_DATA);
    assert_eq!(output_class_of(&types[3]), O_ERROR);

    // A stream's `name` and its `text` representation (here: lines, never joined).
    let s = code.outputs[0];
    assert_eq!(
        notebook::member_string_value(&m, NB_MAIN, s, "name")
            .unwrap()
            .unwrap(),
        "stdout"
    );
    let s_text = notebook::object_member(&m, NB_MAIN, s, "text")
        .unwrap()
        .unwrap();
    assert_eq!(m.node(s_text).unwrap().kind, K_ARRAY);

    // An execute_result's `data` is preserved as an object (its keys reported).
    let e = code.outputs[1];
    let data = notebook::member_keys(&m, NB_MAIN, e, "data")
        .unwrap()
        .unwrap();
    assert_eq!(data, vec!["text/plain"]);

    // An error's `ename`/`evalue`/`traceback` are preserved.
    let er = code.outputs[3];
    assert_eq!(
        notebook::member_string_value(&m, NB_MAIN, er, "ename")
            .unwrap()
            .unwrap(),
        "ValueError"
    );
    let tb = notebook::object_member(&m, NB_MAIN, er, "traceback")
        .unwrap()
        .unwrap();
    let tbn = m.node(tb).unwrap();
    assert_eq!(tbn.kind, K_ARRAY);
    assert_eq!(tbn.children.len(), 2);
    // A display_data output's `data` is preserved too.
    let d = code.outputs[2];
    assert_eq!(
        notebook::member_keys(&m, NB_MAIN, d, "data")
            .unwrap()
            .unwrap(),
        vec!["image/png"]
    );
    // The exact `output_type` token of the stream output.
    let ot = notebook::object_member(&m, NB_MAIN, s, "output_type")
        .unwrap()
        .unwrap();
    let otn = m.node(ot).unwrap();
    assert_eq!(otn.kind, K_STRING);
    assert_eq!(notebook::token_bytes(NB_MAIN, otn).unwrap(), b"\"stream\"");
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", NB_MAIN);
    let field = fx.report.field;
    assert_eq!(fx.report.format, DocumentFormat::Notebook);

    // Native: the nbformat (Text is the decimal, ExactBytes the token).
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::NotebookNbformat,
            Representation::Text
        )),
        "4"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::NotebookNbformat,
            Representation::ExactBytes
        )),
        b"4"
    );

    // Native: a cell descriptor reports its cell_type, source form, and outputs.
    let cell = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::NotebookCell { index: 1 },
        Representation::Metadata,
    ));
    assert!(cell.contains("\"cell_type\":\"code\""), "{cell}");
    assert!(cell.contains("\"source_form\":\"string\""), "{cell}");
    assert!(cell.contains("\"execution_count\":7"), "{cell}");
    assert!(cell.contains("\"outputs\":4"), "{cell}");

    // Native: the exact cell_type string.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::NotebookCellType { index: 0 },
            Representation::Text
        )),
        "markdown"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::NotebookCellType { index: 0 },
            Representation::ExactBytes
        )),
        b"\"markdown\""
    );

    // Native: a string source is decoded; a line-array source is the canonical array.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::NotebookCellSource { index: 1 },
            Representation::Text
        )),
        "print(1 + 1)\n"
    );
    let md_src = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::NotebookCellSource { index: 0 },
        Representation::Text,
    ));
    assert_eq!(md_src, "[\"# Title\\n\",\"Some *text*.\\n\"]");
    // The source form is reported honestly, and its elements are counted.
    let md_meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::NotebookCellSource { index: 0 },
        Representation::Metadata,
    ));
    assert!(md_meta.contains("\"form\":\"lines\""), "{md_meta}");
    assert!(md_meta.contains("\"elements\":2"), "{md_meta}");

    // Native: each output type's descriptor.
    let stream = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::NotebookCellOutput { cell: 1, index: 0 },
        Representation::Metadata,
    ));
    assert!(stream.contains("\"output_type\":\"stream\""), "{stream}");
    assert!(stream.contains("\"name\":\"stdout\""), "{stream}");
    assert!(stream.contains("\"text_form\":\"lines\""), "{stream}");
    let exec = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::NotebookCellOutput { cell: 1, index: 1 },
        Representation::Metadata,
    ));
    assert!(
        exec.contains("\"output_type\":\"execute_result\""),
        "{exec}"
    );
    assert!(exec.contains("\"data_keys\":[\"text/plain\"]"), "{exec}");
    let display = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::NotebookCellOutput { cell: 1, index: 2 },
        Representation::Metadata,
    ));
    assert!(
        display.contains("\"output_type\":\"display_data\""),
        "{display}"
    );
    let err = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::NotebookCellOutput { cell: 1, index: 3 },
        Representation::Metadata,
    ));
    assert!(err.contains("\"output_type\":\"error\""), "{err}");
    assert!(err.contains("\"ename\":\"ValueError\""), "{err}");
    assert!(err.contains("\"traceback\":2"), "{err}");

    // Native: notebook-find reuses the JSON match vocabulary.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::NotebookFind {
            pattern: "output_type".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"key\""), "{found}");

    // Common: metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"notebook\""), "{meta}");
    assert!(meta.contains("\"nbformat\":4"), "{meta}");
    assert!(meta.contains("\"cells\":3"), "{meta}");
    assert!(meta.contains("\"outputs\":4"), "{meta}");
    let _ = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    let _ = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("cell_type".to_string()),
        Representation::Text,
    ));
}

#[test]
fn exact_materialization_is_byte_identical() {
    for src in [
        NB_MAIN,
        NB_V3,
        NB_MIN,
        PLAIN_JSON,
        CELLS_ONLY,
        CELLS_NOT_OBJECTS,
        BAD_NBFORMAT,
        FLOAT_NBFORMAT,
        PROSE,
    ] {
        let fx = Fixture::new("exact", src);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let out = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(out.len(), fx.source.len());
        assert_eq!(sha256(&out), sha256(&fx.source));
        assert_eq!(out, fx.source);
    }
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("doc.ipynb");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, NB_MAIN).unwrap();
    std::fs::write(&desc_path, opaque_descriptor(NB_MAIN)).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &opaque_descriptor(NB_MAIN), Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        Selector::NotebookCellSource { index: 1 },
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"\"print(1 + 1)\\n\"");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), NB_MAIN.len());
    assert_eq!(sha256(&exact), sha256(NB_MAIN));
    assert_eq!(exact, NB_MAIN);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn malformed_inputs_decline_typed() {
    use vole_document::adapter::notebook;
    // Valid JSON, but not nbformat-shaped.
    for src in [
        &b"{\"nbformat\":4}"[..],
        &b"{\"nbformat\":4,\"cells\":[1]}"[..],
        &b"{\"nbformat\":4,\"cells\":[{\"cell_type\":5}]}"[..],
        &b"{\"nbformat\":4,\"cells\":[{\"cell_type\":\"code\",\"source\":5}]}"[..],
        &b"{\"nbformat\":4,\"cells\":[{\"cell_type\":\"code\",\"outputs\":[1]}]}"[..],
        &b"{\"nbformat\":0,\"cells\":[]}"[..],
        &b"[]"[..],
    ] {
        let e = notebook::parse(src, Limits::DEFAULT, true).unwrap_err();
        assert_eq!(
            e.class(),
            ErrorClass::InvalidNotebookStructure,
            "src={src:?}"
        );
    }
    // Malformed JSON is a typed decline too (never a panic).
    let e = notebook::parse(b"{", Limits::DEFAULT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidNotebookStructure);

    // Over the cell cap declines typed (resource limit, not invalid).
    let mut big = Vec::new();
    big.extend_from_slice(b"{\"nbformat\":4,\"cells\":[");
    for i in 0..5000 {
        if i > 0 {
            big.push(b',');
        }
        big.extend_from_slice(b"{\"cell_type\":\"code\",\"source\":\"x\"}");
    }
    big.extend_from_slice(b"]}");
    let e = notebook::parse(&big, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    assert_ne!(
        detect_document_format(&big, Limits::STRICT),
        DocumentFormat::Notebook
    );
    // But it is still valid JSON, so it stays `Json`.
    assert_eq!(
        detect_document_format(&big, Limits::STRICT),
        DocumentFormat::Json
    );
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let fa = ingest_pdf(&mut store, &opaque_descriptor(NB_V3), Limits::DEFAULT)
        .unwrap()
        .field;
    let fb = ingest_pdf(&mut store, &opaque_descriptor(NB_MAIN), Limits::DEFAULT)
        .unwrap()
        .field;

    for (field, src, want) in [(fa, NB_V3, "3".to_string()), (fb, NB_MAIN, "4".to_string())] {
        let got = answer_text(&observe_ok(
            &mut store,
            &field,
            Selector::NotebookNbformat,
            Representation::Text,
        ));
        assert_eq!(got, want, "aliased source for {field:?}");
        let field_handle = Field::open(&store, &field, Limits::DEFAULT).unwrap();
        assert_eq!(
            field_handle.materialize_exact(Limits::DEFAULT).unwrap(),
            src
        );
    }
    drop(store);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn notebook_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived notebook model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    // This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", NB_MAIN);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_NOTEBOOK_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::NotebookModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the notebook model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x4E42_0C24_2026_2400;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::notebook::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::notebook::build_notebook_model(&buf, Limits::STRICT);
            let _ = vole_document::adapter::notebook::canonical_text(&m, &buf);
        }
    }
    // Notebook-shaped hostile inputs must also never panic.
    for src in [
        b"{\"nbformat\":4,\"cells\":[{\"cell_type\":\"".as_slice(),
        b"{\"nbformat\":4,\"cells\":[{\"cell_type\":\"code\",\"outputs\":[{\"output_type\":\""
            .as_slice(),
        b"{\"nbformat\":4,\"cells\":[{\"cell_type\":\"code\",\"source\":[\"".as_slice(),
    ] {
        let _ = detect_document_format(src, Limits::STRICT);
        let _ = vole_document::adapter::notebook::parse(src, Limits::STRICT, true);
    }
}
