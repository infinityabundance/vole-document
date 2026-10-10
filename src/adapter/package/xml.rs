//! Re-export shim for the shared bounded-XML policy.
//!
//! The policy itself now lives in [`crate::adapter::xml_policy`] (Phase 21.9), so
//! the standalone XML adapter — gated on the `xml` feature alone — can reuse it
//! without depending on the `package` feature. Every existing
//! `crate::adapter::package::xml::*` path keeps working through this re-export, so
//! the OPC/EPUB/ODF/DOCX/XLSX/PPTX call sites are untouched.

pub(crate) use crate::adapter::xml_policy::*;
