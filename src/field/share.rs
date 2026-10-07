//! Finer-than-object shareable units (Phase 11.14).
//!
//! Phase 9 externalized the *object table* only
//! ([`ObjectSource::External`], `EXTERNAL_REF`). For most PDF descriptors the
//! auto-complete-cost winner codes its bulk in `ENTROPY_CHANNEL` payloads and
//! program `INLINE` bytes, so the object table is empty and there is nothing to
//! share (ADR-0021). This module takes the next smaller step named in
//! `FINDINGS.md` §7: it names and measures the *fine units* a descriptor is
//! built from, at the granularity below a whole DRA object.
//!
//! ## What is measured, and what is actually externalized
//!
//! [`extract_units`] enumerates four kinds of unit and content-addresses each
//! with `BLAKE3-256`:
//!
//! | kind | unit | source |
//! |---|---|---|
//! | `object` | one object-table entry's raw bytes | `OBJECT` / `EXTERNAL_REF` |
//! | `channel_payload` | an entropy channel's renormalization payload | `ENTROPY_CHANNEL` |
//! | `channel_header` | a channel's fixed 33-byte header (coder/state/counts) | `ENTROPY_CHANNEL` |
//! | `model` | a canonical entropy model's encoded bytes | `MODEL` |
//!
//! Two of these are *truly externalized at the wire level*: the `object` units,
//! through the Phase-9 `EXTERNAL_REF` mechanism ([`externalize_objects`]). The
//! other three are **stored and measured, not wire-externalized**: they are
//! written into the same content-addressed share namespace
//! ([`store_units`]) so their sharing is real and reproducible, but the
//! descriptor still carries them inline. Resolving a channel payload out of the
//! store during materialization would require a new descriptor record (a
//! representation change), which this subphase deliberately does **not**
//! attempt. `cohort_report` therefore reports a *potential* unique-data figure
//! for those kinds; it is never claimed as an achieved store size.
//!
//! ## Accounting discipline (inherited from ADR-0020)
//!
//! A store root is never a whole document. [`ShareReport::unique_bytes`] counts
//! only unique *unit* bytes: it excludes all record framing, the universe, the
//! graph, and the integrity manifest. It is a lower bound on any store form and
//! is only ever compared to a per-file compressor ladder and to generic CDC,
//! never to a single file.

use std::collections::BTreeMap;
use std::path::Path;

use crate::container::{Descriptor, ObjectSource};
use crate::error::Result;
use crate::store::{EmbeddedStore, Id, ObjectStore, externalize};

/// Unit kind: one object-table entry's raw bytes.
pub const KIND_OBJECT: &str = "object";
/// Unit kind: one entropy channel's renormalization payload.
pub const KIND_CHANNEL_PAYLOAD: &str = "channel_payload";
/// Unit kind: one entropy channel's fixed 33-byte header.
pub const KIND_CHANNEL_HEADER: &str = "channel_header";
/// Unit kind: one canonical entropy model's encoded bytes.
pub const KIND_MODEL: &str = "model";

/// One shareable unit: its kind, `BLAKE3-256` content id, and exact byte length.
///
/// `id` is the same ephemeral relational namespace [`Id`] uses; it is never a
/// substitute for the whole-source `SHA-256`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShareUnit {
    /// Unit kind (one of the `KIND_*` constants).
    pub kind: &'static str,
    /// `BLAKE3-256(bytes)` of the unit.
    pub id: [u8; 32],
    /// Exact unit length in bytes.
    pub len: u64,
}

/// Aggregate sharing of the fine units over a cohort of descriptors.
///
/// `total_bytes` sums every unit occurrence; `unique_bytes` sums each distinct
/// unit id once. `by_kind` is `(kind, total, unique)` per kind, sorted by kind
/// name. A distinct unit id belongs to exactly the kinds it appears under, so
/// the per-kind unique column can sum to more than `unique_bytes` when identical
/// bytes occur under two kinds (for example an object and a channel payload).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShareReport {
    /// Sum of every unit occurrence's length.
    pub total_bytes: u64,
    /// Sum of length over distinct unit ids.
    pub unique_bytes: u64,
    /// Number of unit occurrences.
    pub unit_count: u64,
    /// Number of distinct unit ids.
    pub unique_count: u64,
    /// `(kind, total, unique)` per kind, sorted by kind.
    pub by_kind: Vec<(String, u64, u64)>,
}

impl ShareReport {
    /// Render the report as a single JSON object (no serde; stable field order).
    pub fn to_json(&self) -> String {
        let kinds: Vec<String> = self
            .by_kind
            .iter()
            .map(|(k, t, u)| format!("{{\"kind\":\"{k}\",\"total\":{t},\"unique\":{u}}}"))
            .collect();
        format!(
            concat!(
                "{{",
                "\"total_bytes\":{},",
                "\"unique_bytes\":{},",
                "\"unit_count\":{},",
                "\"unique_count\":{},",
                "\"by_kind\":[{}]",
                "}}"
            ),
            self.total_bytes,
            self.unique_bytes,
            self.unit_count,
            self.unique_count,
            kinds.join(",")
        )
    }
}

/// Content-address `bytes` as a unit of `kind`.
fn unit(kind: &'static str, bytes: &[u8]) -> ShareUnit {
    ShareUnit {
        kind,
        id: *Id::of(bytes).as_bytes(),
        len: bytes.len() as u64,
    }
}

/// Extract every fine shareable unit of one descriptor, in a fixed order:
/// models, then per channel its header and payload, then object-table entries.
///
/// Object entries that are already [`ObjectSource::External`] contribute their
/// declared store id and length without touching a store, so extraction works on
/// an externalized descriptor with no resolver.
pub fn extract_units(descriptor: &Descriptor) -> Result<Vec<ShareUnit>> {
    let mut units = Vec::new();
    for model in &descriptor.models {
        let bytes = model.encode()?;
        units.push(unit(KIND_MODEL, &bytes));
    }
    for channel in &descriptor.channels {
        units.push(unit(KIND_CHANNEL_HEADER, &channel.header_bytes()?));
        units.push(unit(KIND_CHANNEL_PAYLOAD, &channel.payload));
    }
    for obj in &descriptor.objects {
        match obj {
            ObjectSource::Inline(bytes) => units.push(unit(KIND_OBJECT, bytes)),
            ObjectSource::External { id, len } => units.push(ShareUnit {
                kind: KIND_OBJECT,
                id: *id.as_bytes(),
                len: *len,
            }),
        }
    }
    Ok(units)
}

/// Aggregate the fine-unit sharing over a cohort of descriptors.
///
/// Deterministic: independent of input order for every headline field (`total`,
/// `unique`, counts, and the `by_kind` table), because units are deduplicated in
/// a `BTreeMap` keyed by content id and the per-kind table is sorted by name.
pub fn cohort_report(descriptors: &[Descriptor]) -> Result<ShareReport> {
    let mut report = ShareReport::default();
    // id -> first-seen length; a content id fixes its length, so any length works.
    let mut seen: BTreeMap<[u8; 32], u64> = BTreeMap::new();
    // kind -> total occurrence bytes
    let mut kind_total: BTreeMap<&'static str, u64> = BTreeMap::new();
    // kind -> (id -> length)
    let mut kind_unique: BTreeMap<&'static str, BTreeMap<[u8; 32], u64>> = BTreeMap::new();

    for descriptor in descriptors {
        for u in extract_units(descriptor)? {
            report.total_bytes += u.len;
            report.unit_count += 1;
            *kind_total.entry(u.kind).or_default() += u.len;
            kind_unique.entry(u.kind).or_default().insert(u.id, u.len);
            if seen.insert(u.id, u.len).is_none() {
                report.unique_bytes += u.len;
                report.unique_count += 1;
            }
        }
    }

    report.by_kind = kind_total
        .into_iter()
        .map(|(kind, total)| {
            let unique: u64 = kind_unique.get(kind).map(|m| m.values().sum()).unwrap_or(0);
            (kind.to_string(), total, unique)
        })
        .collect();
    Ok(report)
}

/// Open the fine-unit content-addressed namespace `<field_root>/share`
/// (an [`EmbeddedStore`], raw and transparent).
pub fn share_store(field_root: &Path) -> Result<EmbeddedStore> {
    EmbeddedStore::open(field_root.join("share"))
}

/// **Truly externalize** the descriptor's object table into `<field_root>/share`.
///
/// Reuses the Phase-9 mechanism unchanged: every object's bytes are put into the
/// content-addressed store and the entry becomes an [`ObjectSource::External`]
/// reference, so `materialize_with(descriptor, &store)` resolves it and stays
/// byte-identical. Returns the number of object-table entries externalized.
pub fn externalize_objects(field_root: &Path, descriptor: &mut Descriptor) -> Result<u64> {
    let mut store = share_store(field_root)?;
    let resolver = store.clone();
    externalize(descriptor, &resolver, &mut store)?;
    Ok(descriptor.objects.len() as u64)
}

/// **Store and measure** every inline fine unit of `descriptor` in the share
/// namespace (models, channel headers, channel payloads, inline objects).
///
/// This is real storage but *not* wire externalization: the descriptor keeps
/// these units inline, so this call changes no descriptor bytes and no exactness
/// property. It exists so the fine-unit sharing is durable and inspectable, and
/// so the distinguishing experiment rests on stored bytes rather than a paper
/// calculation. Returns the number of units offered (identical units collapse by
/// content addressing).
pub fn store_units(field_root: &Path, descriptor: &Descriptor) -> Result<u64> {
    let mut store = share_store(field_root)?;
    let mut offered: u64 = 0;
    for model in &descriptor.models {
        store.put(&model.encode()?)?;
        offered += 1;
    }
    for channel in &descriptor.channels {
        store.put(&channel.header_bytes()?)?;
        store.put(&channel.payload)?;
        offered += 2;
    }
    for obj in &descriptor.objects {
        if let ObjectSource::Inline(bytes) = obj {
            store.put(bytes)?;
            offered += 1;
        }
    }
    Ok(offered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::UNIVERSE;
    use crate::dra::Program;
    use crate::entropy::codec::{
        CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor,
    };
    use crate::entropy::model::EntropyModel;
    use crate::{SOURCE_FORMAT_OPAQUE, integrity};

    fn channel(
        payload: Vec<u8>,
        initial_state: u32,
        decoded_length: u64,
    ) -> EntropyChannelDescriptor {
        EntropyChannelDescriptor {
            coder: CODER_ORDER0_BYTE_RANS,
            coder_version: CODER_VERSION_1,
            scale_bits: 8,
            lane_count: 1,
            model_id: 0,
            symbol_count: decoded_length,
            decoded_length,
            initial_state,
            payload,
        }
    }

    fn descriptor(
        objects: Vec<ObjectSource>,
        channels: Vec<EntropyChannelDescriptor>,
    ) -> Descriptor {
        Descriptor {
            universe: UNIVERSE.to_string(),
            source_format: SOURCE_FORMAT_OPAQUE,
            format_basis: "opaque:share-test".to_string(),
            models: vec![EntropyModel::uniform(8).unwrap()],
            channels,
            objects,
            program: Program::new(vec![]),
            observation_index: None,
            seek_directory: false,
            checkpoints: None,
            source_sha256: [0u8; 32],
            source_len: 0,
        }
    }

    #[test]
    fn repeated_objects_are_counted_twice_and_unique_once() {
        let payload = b"the same object bytes".to_vec();
        let distinct = b"distinct".to_vec();
        let d = descriptor(
            vec![
                ObjectSource::Inline(payload.clone()),
                ObjectSource::Inline(payload.clone()),
                ObjectSource::Inline(distinct.clone()),
            ],
            vec![],
        );
        let report = cohort_report(std::slice::from_ref(&d)).unwrap();
        // 1 model + 3 objects.
        assert_eq!(report.unit_count, 4);
        // The canonical model encoding length is not one byte; derive it.
        let model_len = EntropyModel::uniform(8).unwrap().encode().unwrap().len() as u64;
        let distinct_len = distinct.len() as u64;
        assert_eq!(
            report.total_bytes,
            model_len + 2 * payload.len() as u64 + distinct_len
        );
        assert_eq!(report.unique_count, 3);
        assert_eq!(
            report.unique_bytes,
            model_len + payload.len() as u64 + distinct_len
        );
        assert!(report.unique_bytes <= report.total_bytes);
    }

    #[test]
    fn identical_channel_payloads_share_the_payload_not_the_header() {
        let payload = vec![7u8; 64];
        let a = channel(payload.clone(), 1, 64);
        let b = channel(payload, 2, 64);
        let d = descriptor(vec![], vec![a, b]);
        let report = cohort_report(std::slice::from_ref(&d)).unwrap();
        let by = |kind: &str| {
            report
                .by_kind
                .iter()
                .find(|(k, _, _)| k == kind)
                .cloned()
                .unwrap()
        };
        let (_, payload_total, payload_unique) = by(KIND_CHANNEL_PAYLOAD);
        assert_eq!(payload_total, 128);
        assert_eq!(payload_unique, 64, "identical payloads share once");
        let (_, header_total, header_unique) = by(KIND_CHANNEL_HEADER);
        assert_eq!(header_total, 2 * 33);
        assert_eq!(
            header_unique,
            2 * 33,
            "distinct initial_state -> distinct header"
        );
    }

    #[test]
    fn identical_headers_share_when_only_payload_differs() {
        let a = channel(vec![1u8; 16], 9, 16);
        let b = channel(vec![2u8; 16], 9, 16);
        let d = descriptor(vec![], vec![a, b]);
        let report = cohort_report(std::slice::from_ref(&d)).unwrap();
        let header = report
            .by_kind
            .iter()
            .find(|(k, _, _)| k == KIND_CHANNEL_HEADER)
            .unwrap();
        assert_eq!(header.1, 66);
        assert_eq!(header.2, 33, "equal-length payloads -> one shared header");
    }

    #[test]
    fn report_is_deterministic_and_order_independent() {
        let d1 = descriptor(
            vec![ObjectSource::Inline(b"alpha".to_vec())],
            vec![channel(vec![1, 2, 3], 1, 3)],
        );
        let d2 = descriptor(
            vec![ObjectSource::Inline(b"alpha".to_vec())],
            vec![channel(vec![4, 5, 6], 2, 3)],
        );
        let a = cohort_report(&[d1.clone(), d2.clone()]).unwrap();
        let b = cohort_report(&[d2, d1]).unwrap();
        assert_eq!(a, b);
        assert!(a.unique_bytes <= a.total_bytes);
    }

    #[test]
    fn external_objects_contribute_id_and_length_without_a_store() {
        let inline = descriptor(vec![ObjectSource::Inline(b"0123456789".to_vec())], vec![]);
        let id = Id::of(b"0123456789");
        let external = descriptor(vec![ObjectSource::External { id, len: 10 }], vec![]);
        assert_eq!(
            extract_units(&inline).unwrap(),
            extract_units(&external).unwrap()
        );
    }

    #[test]
    fn object_ids_and_source_sha_are_distinct_namespaces() {
        // A ShareUnit id is a relational content id, never the whole-source digest.
        let bytes = b"namespace check";
        let u = unit(KIND_OBJECT, bytes);
        assert_eq!(u.id, *Id::of(bytes).as_bytes());
        assert_ne!(u.id, integrity::sha256(bytes));
    }

    #[test]
    fn store_units_persists_every_unique_unit_exactly_once() {
        // Two descriptors that share one object and one channel payload: the
        // `share-account` invariant is that the physical store holds exactly the
        // report's unique units, so `store_bytes == unique_bytes` and the store's
        // object count equals the report's unique count.
        let root = std::env::temp_dir().join(format!(
            "vole-share-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let shared_payload = vec![9u8; 32];
        let a = descriptor(
            vec![ObjectSource::Inline(b"shared-object".to_vec())],
            vec![channel(shared_payload.clone(), 1, 32)],
        );
        let b = descriptor(
            vec![ObjectSource::Inline(b"shared-object".to_vec())],
            vec![channel(shared_payload, 1, 32)],
        );
        let offered = store_units(&root, &a).unwrap() + store_units(&root, &b).unwrap();
        assert_eq!(
            offered, 8,
            "2 descriptors x (model + header + payload + object)"
        );
        let report = cohort_report(&[a, b]).unwrap();
        let stats = share_store(&root).unwrap().stats().unwrap();
        assert_eq!(stats.stored_bytes, report.unique_bytes);
        assert_eq!(stats.object_count, report.unique_count);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn externalize_objects_resolves_and_stays_exact() {
        let root = std::env::temp_dir().join(format!(
            "vole-share-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = b"externalized exact bytes";
        let mut d = Descriptor {
            universe: UNIVERSE.to_string(),
            source_format: SOURCE_FORMAT_OPAQUE,
            format_basis: "opaque:share-test".to_string(),
            models: vec![],
            channels: vec![],
            objects: vec![
                ObjectSource::Inline(source.to_vec()),
                ObjectSource::Inline(source.to_vec()),
            ],
            program: Program::new(vec![crate::dra::Op::EmitObject { object_id: 0 }]),
            observation_index: None,
            seek_directory: false,
            checkpoints: None,
            source_sha256: integrity::sha256(source),
            source_len: source.len() as u64,
        };
        let offered = store_units(&root, &d).unwrap();
        assert_eq!(offered, 2);
        externalize_objects(&root, &mut d).unwrap();
        assert!(
            d.objects
                .iter()
                .all(|o| matches!(o, ObjectSource::External { .. }))
        );
        let (bytes, _) = d.serialize().unwrap();
        let parsed = Descriptor::parse(&bytes, crate::limits::Limits::DEFAULT).unwrap();
        let store = share_store(&root).unwrap();
        let out =
            crate::materialize::materialize_with(&parsed, &store, crate::limits::Limits::DEFAULT)
                .unwrap();
        assert_eq!(out, source);
        std::fs::remove_dir_all(&root).ok();
    }
}
