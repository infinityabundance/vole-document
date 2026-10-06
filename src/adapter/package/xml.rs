//! Bounded XML policy shared by the OPC (Phase 12.3) and EPUB (Phase 12.5) cores.
//!
//! XML parsing in this crate is **derived (`Q_gen`) state only**: exact authority
//! remains the Phase-12.2 ZIP member raw spans, and a parsed document never feeds an
//! exact node. The policy is one place so both adapters harden XML identically
//! (research E §2 / I §2–§4):
//!
//! * `quick-xml` with `default-features = false` (UTF-8 only, no `encoding_rs`),
//! * a hard refusal of `<!DOCTYPE` (no DTD, no internal/external entity expansion,
//!   so no XXE and no billion-laughs), and
//! * explicit depth / event / node / attribute / text / size bounds supplied by
//!   [`crate::limits::Limits`].
//!
//! Gated behind the non-default `xml` feature, which both `opc` and `epub` enable.
//! It adds **no** decoder behavior and never touches the exactness path.

use quick_xml::XmlVersion;
use quick_xml::events::BytesStart;

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Reject constructs that must never reach the parser-backed path: over-large
/// parts, UTF-16 encodings, NUL bytes, and non-UTF-8 bytes.
pub(crate) fn harden_xml(bytes: &[u8], limits: Limits) -> Result<()> {
    if bytes.len() as u64 > limits.max_xml_part_bytes {
        return Err(Error::resource_limit("XML part exceeds max_xml_part_bytes"));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(Error::invalid_xml_structure(
            "UTF-16 XML parts are not supported (UTF-8 only)",
        ));
    }
    if bytes.contains(&0) {
        return Err(Error::invalid_xml_structure("XML part contains a NUL byte"));
    }
    if core::str::from_utf8(bytes).is_err() {
        return Err(Error::invalid_xml_structure("XML part is not valid UTF-8"));
    }
    Ok(())
}

/// The typed error for a malformed `quick-xml` event.
pub(crate) fn xml_err(e: quick_xml::Error) -> Error {
    Error::invalid_xml_structure(format!("malformed XML: {e}"))
}

/// The typed error for a forbidden `<!DOCTYPE`.
pub(crate) fn doctype_declined() -> Error {
    Error::invalid_xml_structure("DOCTYPE is forbidden in a package part")
}

/// Read an element's attributes once, bounded by `max_xml_attrs_per_element`,
/// unescaping the five predefined/numeric character references.
pub(crate) fn read_attrs(e: &BytesStart<'_>, limits: Limits) -> Result<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();
    for attr in e.attributes() {
        if out.len() as u64 >= u64::from(limits.max_xml_attrs_per_element) {
            return Err(Error::resource_limit("XML element has too many attributes"));
        }
        let attr =
            attr.map_err(|err| Error::invalid_xml_structure(format!("bad attribute: {err}")))?;
        let key = attr.key.local_name().as_ref().to_string();
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|err| Error::invalid_xml_structure(format!("bad attribute value: {err}")))?
            .into_owned();
        out.push((key, value));
    }
    Ok(out)
}

/// The value of the first attribute named `name` (namespace prefix stripped).
pub(crate) fn attr_of<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

/// Per-part XML budget: events, element nodes, depth, and accumulated text.
pub(crate) struct XmlState {
    events: u64,
    nodes: u64,
    depth: u64,
    text: u64,
}

impl XmlState {
    /// A fresh per-part budget.
    pub(crate) fn new() -> Self {
        XmlState {
            events: 0,
            nodes: 0,
            depth: 0,
            text: 0,
        }
    }

    /// Charge one pull event.
    pub(crate) fn event(&mut self, limits: Limits) -> Result<()> {
        self.events = self.events.saturating_add(1);
        if self.events > limits.max_xml_events {
            return Err(Error::resource_limit("XML event bound exceeded"));
        }
        Ok(())
    }

    /// Charge an element start.
    pub(crate) fn open(&mut self, limits: Limits) -> Result<()> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > limits.max_xml_nodes {
            return Err(Error::resource_limit("XML node bound exceeded"));
        }
        self.depth = self.depth.saturating_add(1);
        if self.depth > u64::from(limits.max_xml_depth) {
            return Err(Error::invalid_xml_structure(
                "XML nesting exceeds max_xml_depth",
            ));
        }
        Ok(())
    }

    /// Charge a self-closing element.
    pub(crate) fn leaf(&mut self, limits: Limits) -> Result<()> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > limits.max_xml_nodes {
            return Err(Error::resource_limit("XML node bound exceeded"));
        }
        Ok(())
    }

    /// Close an element.
    pub(crate) fn close(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Charge accumulated text bytes.
    pub(crate) fn text(&mut self, n: usize, limits: Limits) -> Result<()> {
        self.text = self.text.saturating_add(n as u64);
        if self.text > limits.max_xml_text_bytes {
            return Err(Error::resource_limit("XML text bound exceeded"));
        }
        Ok(())
    }
}
