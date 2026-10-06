//! Complete-cost accounting.
//!
//! A representation is never credited with an entropy estimate. Every byte
//! that must be persisted to make a candidate decodable is charged, and only
//! the complete serialized `.voldoc` size is authoritative for standalone
//! compression claims.

/// Physical byte attribution for a serialized descriptor.
///
/// These categories exist so that `inspect` can answer *where the bytes went*
/// when a mechanism wins or loses. They sum to the exact `.voldoc` length.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CostBreakdown {
    /// Fixed header bytes.
    pub header: u64,
    /// Universe declaration record (payload + framing).
    pub universe: u64,
    /// Format-descriptor record (payload + framing).
    pub format: u64,
    /// Record framing overhead (tag/flags/length/crc) for all records.
    pub record_framing: u64,
    /// Object record payloads (raw byte objects).
    pub objects: u64,
    /// `EXTERNAL_REF` record payloads (Phase 9): 40 bytes per external object.
    ///
    /// The record framing is charged to [`CostBreakdown::record_framing`], as for
    /// `OBJECT` records.
    pub external_refs: u64,
    /// Reconstruction-graph record payload.
    pub graph: u64,
    /// Entropy model records (0 in the exact core).
    pub models: u64,
    /// Entropy channel payloads (0 in the exact core).
    pub entropy_payload: u64,
    /// Typed residual records (0 in the exact core).
    pub residuals: u64,
    /// Index/checkpoint records.
    pub index: u64,
    /// Optional seek-directory record (payload + framing). Zero when absent.
    pub directory: u64,
    /// Integrity record payload.
    pub integrity: u64,
    /// Trailer record payload.
    pub trailer: u64,
}

impl CostBreakdown {
    /// Total serialized size in bytes.
    pub fn total(&self) -> u64 {
        self.header
            + self.universe
            + self.format
            + self.record_framing
            + self.objects
            + self.external_refs
            + self.graph
            + self.models
            + self.entropy_payload
            + self.residuals
            + self.index
            + self.directory
            + self.integrity
            + self.trailer
    }

    /// Stable-order JSON rendering for `inspect` and receipts.
    pub fn to_json(&self) -> String {
        let mut s = String::new();
        s.push('{');
        let fields: [(&str, u64); 14] = [
            ("header", self.header),
            ("universe", self.universe),
            ("format", self.format),
            ("record_framing", self.record_framing),
            ("objects", self.objects),
            ("external_refs", self.external_refs),
            ("graph", self.graph),
            ("models", self.models),
            ("entropy_payload", self.entropy_payload),
            ("residuals", self.residuals),
            ("index", self.index),
            ("directory", self.directory),
            ("integrity", self.integrity),
            ("trailer", self.trailer),
        ];
        for (i, (name, value)) in fields.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push('"');
            s.push_str(name);
            s.push_str("\":");
            s.push_str(&value.to_string());
        }
        s.push('}');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_sums_all_fields() {
        let c = CostBreakdown {
            header: 64,
            objects: 100,
            record_framing: 24,
            ..Default::default()
        };
        assert_eq!(c.total(), 188);
    }

    #[test]
    fn json_has_all_fields() {
        let j = CostBreakdown::default().to_json();
        for k in [
            "header",
            "universe",
            "format",
            "record_framing",
            "objects",
            "external_refs",
            "graph",
            "models",
            "entropy_payload",
            "residuals",
            "index",
            "directory",
            "integrity",
            "trailer",
        ] {
            assert!(j.contains(k), "missing {k}");
        }
    }
}
