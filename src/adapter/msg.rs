//! Bounded, representation-preserving legacy Outlook message (`.msg`, MS-OXMSG)
//! **semantic** adapter (Phase 26.4), built on the CFBF/OLE2 physical layer (21.35).
//!
//! A `.msg` is a CFBF compound file whose **root storage** holds the message's
//! *fixed-size* property list (`__properties_version1.0`), its *variable-size*
//! property streams (`__substg1.0_<PPPPTTTT>`), an optional named-property map
//! (`__nameid_version1.0`), and the recipient/attachment sub-storages
//! (`__recip_version1.0_<id>` / `__attach_version1.0_<id>`), each of which has its
//! own `__properties_version1.0` and `__substg1.0_*` streams. This adapter **reuses
//! the CFBF resolver** ([`crate::adapter::cfbf`]) to obtain every stream as an exact
//! byte span — it never writes a second OLE parser — and adds only a bounded
//! semantic model over the property streams and sub-storages.
//!
//! ## The property stream
//!
//! `__properties_version1.0` opens with a fixed header and is followed by 16-byte
//! property entries:
//!
//! ```text
//! message (top-level) header, 32 bytes:
//!     Reserved            8 bytes  (verbatim)
//!     NextRecipientId     4 bytes  little-endian
//!     NextAttachmentId    4 bytes  little-endian
//!     RecipientCount      4 bytes  little-endian
//!     AttachmentCount     4 bytes  little-endian
//!     Reserved            8 bytes  (verbatim)
//!
//! recipient/attachment storage header, 8 bytes:
//!     Reserved            8 bytes  (verbatim)
//!
//! property entry, 16 bytes:
//!     PropertyTag         4 bytes  little-endian (`0xPPPPTTTT`: id `P`, type `T`)
//!     Flags               4 bytes  little-endian (recorded verbatim)
//!     Value               8 bytes  (value-or-reserved, recorded verbatim)
//! ```
//!
//! ## The variable-size substreams
//!
//! A `__substg1.0_<PPPPTTTT>` stream's 8-digit hex suffix is the owning fixed
//! entry's property tag (`PPPP` the id, `TTTT` the type). This adapter decodes
//! type `0x001F` (`PT_UNICODE`, UTF-16LE), `0x001E` (`PT_STRING8`, ANSI/CP1252),
//! and `0x0102` (`PT_BINARY`) — recording each stream's **exact span** and, for the
//! two text types, its decoded text.
//!
//! ## What is authoritative
//!
//! Like every Wave-2 format, a `.msg` has **no separate package layer**: the exact
//! leaf is the whole source (a `DocumentExact`) and every derived observation is a
//! bounded, span-preserving (`Q_gen`) projection of it. Nothing is re-encoded,
//! re-serialized, or re-OLE'd for exactness.
//!
//! ## What is parsed (the documented subset)
//!
//! * the root-level `__properties_version1.0` stream: its 32-byte header
//!   (`Reserved`, `NextRecipientId`, `NextAttachmentId`, `RecipientCount`,
//!   `AttachmentCount`, `Reserved`) and its 16-byte fixed-size property entries, in
//!   order, with the raw tag (id/type), flags, value bytes, and exact span;
//! * the root-level `__substg1.0_<PPPPTTTT>` variable-size streams, decoded by type;
//! * the recipient storages (`__recip_version1.0_<id>`) and attachment storages
//!   (`__attach_version1.0_<id>`), in directory order, each with its own 8-byte
//!   header, fixed-size property entries, and variable-size substreams;
//! * a static named-property map for the well-known property ids ([`property_name`]),
//!   keeping the raw tag alongside.
//!
//! Stream order, entry order, storage order and ids, each stream's exact span, the
//! decoded text with its type, and the raw tags are all preserved; nothing is
//! normalized.
//!
//! ## The boundary it cannot cross (honest)
//!
//! Detection is built **on top of** the CFBF detector: a source is claimed only when
//! the CFBF layer accepts it **and** it has a root-level `__properties_version1.0`
//! stream. A generic compound file stays
//! [`Cfbf`](crate::field::document_format::DocumentFormat::Cfbf). A `.doc`/`.xls`/
//! `.ppt` is claimed by its own detector first. The seam this leaves is honest: an
//! OLE file that merely *has* a root-level stream with that name (but is not a
//! message) is admitted here, and its derived model then declines **typed** if the
//! stream is not a well-formed message property list.
//!
//! A `.msg` whose `__properties_version1.0` is too short, whose fixed entries do not
//! fill the stream exactly (an entry overruns), whose `__substg1.0_*` name is not
//! `PPPPTTTT` hex, whose declared `RecipientCount`/`AttachmentCount` disagrees with
//! the sub-storages, whose sub-storage lacks its own `__properties_version1.0`, or
//! whose `__substg1.0_*` stream is not referenced by any fixed entry declines
//! **typed** ([`crate::ErrorClass::InvalidMsgStructure`]). Never a panic, never a
//! guess.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model property entries (defends the decoder against a hostile blob).
pub const MAX_MODEL_PROPERTIES: u32 = 1 << 24;
/// Hard cap on decoded model substreams.
pub const MAX_MODEL_SUBSTREAMS: u32 = 1 << 24;
/// Hard cap on decoded model recipient/attachment sub-storages.
pub const MAX_MODEL_SUBSTORAGES: u32 = 1 << 20;
/// Hard cap on a decoded model name length (an MS-OXMSG name is short).
pub const MAX_MODEL_NAME_BYTES: u32 = 1 << 12;
/// Hard cap on decoded text code units.
pub const MAX_MODEL_TEXT_UNITS: u32 = 1 << 30;

/// The root-level fixed-size property stream name.
pub const STREAM_PROPERTIES: &str = "__properties_version1.0";
/// The optional named-property map stream name.
pub const STREAM_NAMEID: &str = "__nameid_version1.0";
/// The variable-size substream name prefix.
pub const SUBSTREAM_PREFIX: &str = "__substg1.0_";
/// The recipient sub-storage name prefix.
pub const RECIPIENT_PREFIX: &str = "__recip_version1.0_";
/// The attachment sub-storage name prefix.
pub const ATTACHMENT_PREFIX: &str = "__attach_version1.0_";

/// The top-level message property-stream header length.
pub const HEADER_LEN_MESSAGE: usize = 32;
/// The recipient/attachment property-stream header length.
pub const HEADER_LEN_STORAGE: usize = 8;
/// One fixed-size property entry's length.
pub const PROPERTY_ENTRY_LEN: usize = 16;

/// Property type `PT_UNICODE` (UTF-16LE).
pub const PT_UNICODE: u16 = 0x001F;
/// Property type `PT_STRING8` (ANSI/CP1252).
pub const PT_STRING8: u16 = 0x001E;
/// Property type `PT_BINARY`.
pub const PT_BINARY: u16 = 0x0102;

/// Match field: a decoded substream.
pub const FIELD_SUBSTREAM: u8 = 0;

/// The stable match-field name.
pub const fn field_name(field: u8) -> &'static str {
    let _ = field;
    "substream"
}

/// Whether a property type is one this adapter decodes (as a variable-size substream).
pub const fn is_supported_type(prop_type: u16) -> bool {
    matches!(prop_type, PT_UNICODE | PT_STRING8 | PT_BINARY)
}

/// A stable name for a property type.
pub const fn property_type_name(prop_type: u16) -> &'static str {
    match prop_type {
        PT_UNICODE => "unicode",
        PT_STRING8 => "string8",
        PT_BINARY => "binary",
        0x0002 => "i2",
        0x0003 => "long",
        0x0004 => "r4",
        0x0005 => "r8",
        0x000A => "error",
        0x000B => "boolean",
        0x000D => "object",
        0x0014 => "i8",
        0x0040 => "systime",
        0x0048 => "clsid",
        _ => "other",
    }
}

/// A stable, human-readable name for a well-known property id (MS-OXPROPS), or
/// `"unknown"`. The raw tag is always kept alongside, so this map never loses
/// information.
pub const fn property_name(id: u16) -> &'static str {
    match id {
        0x001A => "message-class",        // PidTagMessageClass
        0x0037 => "subject",              // PidTagSubject
        0x0039 => "client-submit-time",   // PidTagClientSubmitTime
        0x1000 => "body",                 // PidTagBody
        0x0C1A => "sender-name",          // PidTagSenderName
        0x0E04 => "display-to",           // PidTagDisplayTo
        0x3704 => "attach-filename",      // PidTagAttachFilename
        0x3707 => "attach-long-filename", // PidTagAttachLongFilename
        0x370E => "attach-mime-tag",      // PidTagAttachMimeTag
        _ => "unknown",
    }
}

/// The extracted property id (the high 16 bits of a property tag).
pub const fn tag_id(tag: u32) -> u16 {
    (tag >> 16) as u16
}

/// The extracted property type (the low 16 bits of a property tag).
pub const fn tag_type(tag: u32) -> u16 {
    (tag & 0xFFFF) as u16
}

/// The recorded `__properties_version1.0` header of the top-level message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgHeader {
    /// The leading 8 reserved bytes, verbatim (MUST be zero in a well-formed file).
    pub reserved0: [u8; 8],
    /// `NextRecipientId`.
    pub next_recipient_id: u32,
    /// `NextAttachmentId`.
    pub next_attachment_id: u32,
    /// The declared `RecipientCount`.
    pub recipient_count: u32,
    /// The declared `AttachmentCount`.
    pub attachment_count: u32,
    /// The trailing 8 reserved bytes, verbatim.
    pub reserved1: [u8; 8],
}

/// One fixed-size property entry (16 bytes) with its exact source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgProperty {
    /// The raw property tag (`0xPPPPTTTT`: id high, type low).
    pub tag: u32,
    /// The raw flags, verbatim.
    pub flags: u32,
    /// The 8 value-or-reserved bytes, verbatim.
    pub value: [u8; 8],
    /// `(offset, length)` of the 16-byte entry in the source.
    pub span: (u64, u64),
    /// The directory index of the owning storage (`0` for the top-level message).
    pub owner: u32,
}

impl MsgProperty {
    /// The property id (the high 16 bits of the tag).
    pub const fn id(&self) -> u16 {
        tag_id(self.tag)
    }

    /// The property type (the low 16 bits of the tag).
    pub const fn prop_type(&self) -> u16 {
        tag_type(self.tag)
    }
}

/// One variable-size property substream (`__substg1.0_<PPPPTTTT>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgSubstream {
    /// The verbatim stream name.
    pub name: String,
    /// The property tag parsed from the name suffix.
    pub tag: u32,
    /// The directory index of the stream entry.
    pub dir_index: u32,
    /// The directory index of the owning storage (`0` for the top-level message).
    pub owner: u32,
    /// `(offset, length)` of the stream content in the source.
    pub span: (u64, u64),
    /// The stream's byte length.
    pub len: u64,
    /// The decoded text for `PT_UNICODE`/`PT_STRING8`; `None` for `PT_BINARY`.
    pub text: Option<String>,
}

impl MsgSubstream {
    /// The property id (the high 16 bits of the tag).
    pub const fn id(&self) -> u16 {
        tag_id(self.tag)
    }

    /// The property type (the low 16 bits of the tag).
    pub const fn prop_type(&self) -> u16 {
        tag_type(self.tag)
    }
}

/// One recipient or attachment sub-storage with its own property list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgStorage {
    /// The `id` parsed from the storage name suffix.
    pub id: u32,
    /// The directory index of the storage entry.
    pub dir_index: u32,
    /// `(offset, length)` of the 128-byte storage directory entry in the source.
    pub span: (u64, u64),
    /// `(offset, length)` of the storage's `__properties_version1.0` content.
    pub props_span: (u64, u64),
    /// The storage's fixed-size property entries, in order.
    pub properties: Vec<MsgProperty>,
    /// The storage's variable-size substreams, in directory order.
    pub substreams: Vec<MsgSubstream>,
}

/// The canonical derived legacy-MSG model (the materialization of a `MsgModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// `(offset, length)` of the root `__properties_version1.0` content.
    pub property_span: (u64, u64),
    /// The recorded top-level message header.
    pub header: MsgHeader,
    /// `(offset, length)` of the optional `__nameid_version1.0` stream.
    pub nameid_span: Option<(u64, u64)>,
    /// The top-level message's fixed-size property entries, in order.
    pub properties: Vec<MsgProperty>,
    /// The top-level message's variable-size substreams, in directory order.
    pub substreams: Vec<MsgSubstream>,
    /// The recipient sub-storages, in directory order.
    pub recipients: Vec<MsgStorage>,
    /// The attachment sub-storages, in directory order.
    pub attachments: Vec<MsgStorage>,
}

impl MsgModel {
    /// The top-level property at `index`, if present.
    pub fn property(&self, index: u32) -> Option<&MsgProperty> {
        self.properties.get(index as usize)
    }

    /// The top-level substream at `index`, if present.
    pub fn substream(&self, index: u32) -> Option<&MsgSubstream> {
        self.substreams.get(index as usize)
    }

    /// The recipient at `index`, if present.
    pub fn recipient(&self, index: u32) -> Option<&MsgStorage> {
        self.recipients.get(index as usize)
    }

    /// The attachment at `index`, if present.
    pub fn attachment(&self, index: u32) -> Option<&MsgStorage> {
        self.attachments.get(index as usize)
    }

    /// Every substream in canonical order: the top-level message's, then each
    /// recipient's, then each attachment's.
    pub fn all_substreams(&self) -> Vec<(u32, &MsgSubstream)> {
        let mut out: Vec<(u32, &MsgSubstream)> = Vec::new();
        for s in &self.substreams {
            out.push((0, s));
        }
        for r in &self.recipients {
            for s in &r.substreams {
                out.push((r.dir_index, s));
            }
        }
        for a in &self.attachments {
            for s in &a.substreams {
                out.push((a.dir_index, s));
            }
        }
        out
    }

    /// The aggregate decoded text code-unit count (the byte budget the caller checks
    /// against `max_msg_text_bytes`).
    pub fn text_units(&self) -> u64 {
        self.all_substreams()
            .iter()
            .filter_map(|(_, s)| s.text.as_ref())
            .map(|t| t.chars().count() as u64)
            .sum()
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"MSGM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        w_span(&mut out, self.property_span);
        out.extend_from_slice(&self.header.reserved0);
        out.extend_from_slice(&self.header.next_recipient_id.to_le_bytes());
        out.extend_from_slice(&self.header.next_attachment_id.to_le_bytes());
        out.extend_from_slice(&self.header.recipient_count.to_le_bytes());
        out.extend_from_slice(&self.header.attachment_count.to_le_bytes());
        out.extend_from_slice(&self.header.reserved1);
        match self.nameid_span {
            None => out.push(0),
            Some(s) => {
                out.push(1);
                w_span(&mut out, s);
            }
        }
        w_props(&mut out, &self.properties);
        w_subs(&mut out, &self.substreams);
        w_storages(&mut out, &self.recipients);
        w_storages(&mut out, &self.attachments);
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<MsgModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"MSGM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let doc_len = r.u64()?;
        let property_span = r.span()?;
        check_span(property_span, doc_len, "__properties_version1.0 span")?;
        let mut reserved0 = [0u8; 8];
        reserved0.copy_from_slice(r.bytes(8)?);
        let next_recipient_id = r.u32()?;
        let next_attachment_id = r.u32()?;
        let recipient_count = r.u32()?;
        let attachment_count = r.u32()?;
        let mut reserved1 = [0u8; 8];
        reserved1.copy_from_slice(r.bytes(8)?);
        let header = MsgHeader {
            reserved0,
            next_recipient_id,
            next_attachment_id,
            recipient_count,
            attachment_count,
            reserved1,
        };
        let nameid_span = if r.u8()? != 0 {
            let s = r.span()?;
            check_span(s, doc_len, "__nameid_version1.0 span")?;
            Some(s)
        } else {
            None
        };
        let properties = r_props(&mut r, doc_len)?;
        let substreams = r_subs(&mut r, doc_len)?;
        let recipients = r_storages(&mut r, doc_len)?;
        let attachments = r_storages(&mut r, doc_len)?;
        if recipients.len() as u64 != recipient_count as u64 {
            return Err(corrupt("decoded recipient count disagrees with the header"));
        }
        if attachments.len() as u64 != attachment_count as u64 {
            return Err(corrupt(
                "decoded attachment count disagrees with the header",
            ));
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(MsgModel {
            doc_len,
            property_span,
            header,
            nameid_span,
            properties,
            substreams,
            recipients,
            attachments,
        })
    }
}

fn check_span(span: (u64, u64), bound: u64, what: &'static str) -> Result<()> {
    if span.0 > span.1 || span.1 > bound {
        return Err(corrupt(format!("{what} is outside its container")));
    }
    Ok(())
}

/// A lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgMatch {
    /// [`FIELD_SUBSTREAM`].
    pub field: u8,
    /// The global substream ordinal (top-level, then recipients, then attachments).
    pub index: u32,
    /// The owning storage's directory index (`0` for the top-level message).
    pub owner: u32,
    /// The substream content's first byte offset in the source.
    pub start: u64,
    /// One past the substream content's last byte in the source.
    pub end: u64,
    /// The matched substream's decoded text.
    pub text: String,
}

/// A bounded, case-sensitive lexical search over each decoded substream text.
pub fn find(model: &MsgModel, pattern: &str, max_bytes: u64) -> Result<Vec<MsgMatch>> {
    if pattern.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<MsgMatch> = Vec::new();
    let mut estimated: u64 = 0;
    for (i, (owner, s)) in model.all_substreams().iter().enumerate() {
        let Some(text) = &s.text else {
            continue;
        };
        if text.contains(pattern) {
            estimated = estimated.saturating_add(64 + text.len() as u64);
            if estimated > max_bytes {
                return Err(Error::resource_limit(format!(
                    "msg find exceeded the {max_bytes}-byte budget"
                )));
            }
            out.push(MsgMatch {
                field: FIELD_SUBSTREAM,
                index: i as u32,
                owner: *owner,
                start: s.span.0,
                end: s.span.1,
                text: text.clone(),
            });
        }
    }
    Ok(out)
}

/// The canonical text rendering of the model: the decoded substream texts in
/// canonical order (top-level, then recipients, then attachments). Bounded by
/// `max_bytes`.
pub fn canonical_text(model: &MsgModel, _source: &[u8], max_bytes: u64) -> Result<String> {
    let mut out = String::new();
    for (_, s) in model.all_substreams() {
        let Some(text) = &s.text else {
            continue;
        };
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(text);
        if out.len() as u64 > max_bytes {
            return Err(Error::resource_limit(format!(
                "msg canonical text exceeded the {max_bytes}-byte budget"
            )));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Fast-fail legacy-MSG detector, built **on top of** the CFBF detector: the source
/// must be a compound file the CFBF layer accepts and must have a **root-level**
/// `__properties_version1.0` stream. Never consults a file name.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_msg_document_bytes {
        return false;
    }
    if !crate::adapter::cfbf::detect(source, limits) {
        return false;
    }
    let Ok(model) = crate::adapter::cfbf::parse(source, limits) else {
        return false;
    };
    find_root_stream(&model, STREAM_PROPERTIES).is_some()
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse `source` into a [`MsgModel`] under `limits`. Declines typed on any malformed
/// or over-large input; never panics.
pub fn parse(source: &[u8], limits: Limits) -> Result<MsgModel> {
    let len = source.len() as u64;
    if len > limits.max_msg_document_bytes {
        return Err(Error::resource_limit(format!(
            "msg source is {len} bytes, above the {}-byte document cap",
            limits.max_msg_document_bytes
        )));
    }
    let cfbf = crate::adapter::cfbf::parse(source, limits)?;

    // The root storage is directory entry 0 (guaranteed by the CFBF parser); a
    // root-level stream is a stream whose reconstructed parent is that entry.
    let prop_entry = find_root_stream(&cfbf, STREAM_PROPERTIES).ok_or_else(|| {
        invalid("the compound file has no root-level __properties_version1.0 stream")
    })?;
    let (prop_bytes, prop_span) = stream_exact(source, prop_entry)?;
    let (header, properties) = parse_message_properties(prop_bytes, prop_span.0)?;

    let mut budget = Budget {
        properties: properties.len() as u64,
        ..Budget::default()
    };
    if budget.properties > limits.max_msg_properties {
        return Err(Error::resource_limit(format!(
            "the message has more than {} property entries",
            limits.max_msg_properties
        )));
    }

    let nameid_span = match find_root_stream(&cfbf, STREAM_NAMEID) {
        Some(e) => Some(stream_exact(source, e)?.1),
        None => None,
    };

    // The root-level substreams, in directory order.
    let mut substreams: Vec<MsgSubstream> = Vec::new();
    for (i, e) in cfbf.entries.iter().enumerate() {
        if !is_child_stream(e, ROOT_INDEX) {
            continue;
        }
        let name = crate::adapter::cfbf::entry_name_text(e);
        let Some(suffix) = name.strip_prefix(SUBSTREAM_PREFIX) else {
            continue;
        };
        let sub = parse_substream(
            source,
            e,
            i as u32,
            &name,
            suffix,
            ROOT_INDEX,
            &properties,
            &mut budget,
            limits,
        )?;
        substreams.push(sub);
    }

    // The recipient/attachment sub-storages, in directory order.
    let mut recipients: Vec<MsgStorage> = Vec::new();
    let mut attachments: Vec<MsgStorage> = Vec::new();
    for (i, e) in cfbf.entries.iter().enumerate() {
        if e.obj_type != crate::adapter::cfbf::OBJ_STORAGE || e.parent != ROOT_INDEX {
            continue;
        }
        let name = crate::adapter::cfbf::entry_name_text(e);
        if let Some(suffix) = name.strip_prefix(RECIPIENT_PREFIX) {
            let id = parse_decimal_id(suffix, "recipient")?;
            if recipients.len() as u64 >= limits.max_msg_recipients {
                return Err(Error::resource_limit(format!(
                    "the message has more than {} recipients",
                    limits.max_msg_recipients
                )));
            }
            recipients.push(parse_storage(
                source,
                &cfbf,
                i as u32,
                name,
                id,
                e,
                &mut budget,
                limits,
            )?);
        } else if let Some(suffix) = name.strip_prefix(ATTACHMENT_PREFIX) {
            let id = parse_decimal_id(suffix, "attachment")?;
            if attachments.len() as u64 >= limits.max_msg_attachments {
                return Err(Error::resource_limit(format!(
                    "the message has more than {} attachments",
                    limits.max_msg_attachments
                )));
            }
            attachments.push(parse_storage(
                source,
                &cfbf,
                i as u32,
                name,
                id,
                e,
                &mut budget,
                limits,
            )?);
        }
    }

    // The declared counts must agree with the sub-storages.
    if recipients.len() as u64 != header.recipient_count as u64 {
        return Err(invalid(format!(
            "the header declares {} recipients but the file has {} recipient storages",
            header.recipient_count,
            recipients.len()
        )));
    }
    if attachments.len() as u64 != header.attachment_count as u64 {
        return Err(invalid(format!(
            "the header declares {} attachments but the file has {} attachment storages",
            header.attachment_count,
            attachments.len()
        )));
    }

    if budget.name_bytes > limits.max_msg_name_bytes {
        return Err(Error::resource_limit(format!(
            "msg substream names exceed the {}-byte name cap",
            limits.max_msg_name_bytes
        )));
    }

    Ok(MsgModel {
        doc_len: len,
        property_span: prop_span,
        header,
        nameid_span,
        properties,
        substreams,
        recipients,
        attachments,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the materialization
/// of a `MsgModel` node).
pub fn build_msg_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// The directory index of the root storage.
const ROOT_INDEX: u32 = 0;

/// Whether an entry is a top-level-child stream of `owner`.
fn is_child_stream(e: &crate::adapter::cfbf::CfbfEntry, owner: u32) -> bool {
    e.obj_type == crate::adapter::cfbf::OBJ_STREAM && e.parent == owner
}

/// Find a root-level stream by its verbatim name.
fn find_root_stream<'a>(
    model: &'a crate::adapter::cfbf::CfbfModel,
    name: &str,
) -> Option<&'a crate::adapter::cfbf::CfbfEntry> {
    model.entries.iter().find(|e| {
        e.obj_type == crate::adapter::cfbf::OBJ_STREAM
            && e.parent == ROOT_INDEX
            && crate::adapter::cfbf::entry_name_text(e) == name
    })
}

/// Find a stream that is a direct child of `owner`, by its verbatim name.
fn find_child_stream<'a>(
    model: &'a crate::adapter::cfbf::CfbfModel,
    owner: u32,
    name: &str,
) -> Option<&'a crate::adapter::cfbf::CfbfEntry> {
    model.entries.iter().find(|e| {
        e.obj_type == crate::adapter::cfbf::OBJ_STREAM
            && e.parent == owner
            && crate::adapter::cfbf::entry_name_text(e) == name
    })
}

/// The exact bytes and `(offset, length)` span of a stream. Declines typed for a
/// non-contiguous stream (whose model span is only a bounding hull).
fn stream_exact<'a>(
    source: &'a [u8],
    e: &crate::adapter::cfbf::CfbfEntry,
) -> Result<(&'a [u8], (u64, u64))> {
    let bytes = crate::adapter::cfbf::stream_bytes(source, e)?;
    let span = (e.data_span.0, e.data_span.0.saturating_add(e.stream_size));
    Ok((bytes, span))
}

/// The running bounds checked during a parse.
#[derive(Default)]
struct Budget {
    properties: u64,
    substreams: u64,
    name_bytes: u64,
    value_bytes: u64,
    text_units: u64,
}

/// Parse the top-level message property stream (32-byte header, then 16-byte entries).
fn parse_message_properties(bytes: &[u8], base: u64) -> Result<(MsgHeader, Vec<MsgProperty>)> {
    if bytes.len() < HEADER_LEN_MESSAGE {
        return Err(invalid(
            "__properties_version1.0 is shorter than the 32-byte message header",
        ));
    }
    let mut reserved0 = [0u8; 8];
    reserved0.copy_from_slice(&bytes[0..8]);
    let next_recipient_id = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let next_attachment_id = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    let recipient_count = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let attachment_count = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    let mut reserved1 = [0u8; 8];
    reserved1.copy_from_slice(&bytes[24..32]);
    let header = MsgHeader {
        reserved0,
        next_recipient_id,
        next_attachment_id,
        recipient_count,
        attachment_count,
        reserved1,
    };
    let entries = parse_entries(
        &bytes[HEADER_LEN_MESSAGE..],
        base,
        HEADER_LEN_MESSAGE as u64,
    )?;
    Ok((header, entries))
}

/// Parse a recipient/attachment property stream (8-byte header, then 16-byte entries).
fn parse_storage_properties(
    bytes: &[u8],
    base: u64,
    owner: u32,
    limits: Limits,
) -> Result<Vec<MsgProperty>> {
    if bytes.len() < HEADER_LEN_STORAGE {
        return Err(invalid(
            "a sub-storage __properties_version1.0 is shorter than the 8-byte header",
        ));
    }
    let entries = parse_entries(
        &bytes[HEADER_LEN_STORAGE..],
        base,
        HEADER_LEN_STORAGE as u64,
    )?;
    if entries.len() as u64 > limits.max_msg_properties {
        return Err(Error::resource_limit(format!(
            "a sub-storage has more than {} property entries",
            limits.max_msg_properties
        )));
    }
    Ok(entries
        .into_iter()
        .map(|mut p| {
            p.owner = owner;
            p
        })
        .collect())
}

/// Parse a 16-byte-aligned property-entry region.
fn parse_entries(rest: &[u8], base: u64, header: u64) -> Result<Vec<MsgProperty>> {
    if !rest.len().is_multiple_of(PROPERTY_ENTRY_LEN) {
        return Err(invalid(
            "a fixed property entry overruns the property stream",
        ));
    }
    let n = rest.len() / PROPERTY_ENTRY_LEN;
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let o = k * PROPERTY_ENTRY_LEN;
        let tag = u32::from_le_bytes([rest[o], rest[o + 1], rest[o + 2], rest[o + 3]]);
        let flags = u32::from_le_bytes([rest[o + 4], rest[o + 5], rest[o + 6], rest[o + 7]]);
        let mut value = [0u8; 8];
        value.copy_from_slice(&rest[o + 8..o + 16]);
        let e_off = base + header + o as u64;
        out.push(MsgProperty {
            tag,
            flags,
            value,
            span: (e_off, e_off + PROPERTY_ENTRY_LEN as u64),
            owner: 0,
        });
    }
    Ok(out)
}

/// Parse one `__substg1.0_<PPPPTTTT>` substream.
#[allow(clippy::too_many_arguments)]
fn parse_substream(
    source: &[u8],
    e: &crate::adapter::cfbf::CfbfEntry,
    dir_index: u32,
    name: &str,
    suffix: &str,
    owner: u32,
    properties: &[MsgProperty],
    budget: &mut Budget,
    limits: Limits,
) -> Result<MsgSubstream> {
    let tag = parse_tag_hex(suffix)?;
    if budget.substreams >= limits.max_msg_substreams {
        return Err(Error::resource_limit(format!(
            "the message has more than {} substreams",
            limits.max_msg_substreams
        )));
    }
    // The substream must be referenced by a fixed entry in the same property list.
    if !properties.iter().any(|p| p.tag == tag) {
        return Err(invalid(format!(
            "the substream {name} is not referenced by any fixed property entry"
        )));
    }
    let prop_type = tag_type(tag);
    if !is_supported_type(prop_type) {
        return Err(invalid(format!(
            "the substream {name} has an unsupported property type 0x{prop_type:04X}"
        )));
    }
    let (bytes, span) = stream_exact(source, e)?;
    if span.1.saturating_sub(span.0) != bytes.len() as u64 {
        return Err(invalid("a substream span disagrees with its length"));
    }
    budget.substreams += 1;
    budget.name_bytes = budget.name_bytes.saturating_add(name.len() as u64);
    budget.value_bytes = budget.value_bytes.saturating_add(bytes.len() as u64);
    if budget.value_bytes > limits.max_msg_value_bytes {
        return Err(Error::resource_limit(format!(
            "msg substream values exceed the {}-byte value cap",
            limits.max_msg_value_bytes
        )));
    }
    let text = match prop_type {
        PT_UNICODE => Some(decode_utf16le(bytes)),
        PT_STRING8 => Some(decode_cp1252(bytes)),
        _ => None,
    };
    if let Some(t) = &text {
        budget.text_units = budget.text_units.saturating_add(t.chars().count() as u64);
        if budget.text_units > limits.max_msg_text_bytes {
            return Err(Error::resource_limit(format!(
                "msg derived text is above the {}-byte text cap",
                limits.max_msg_text_bytes
            )));
        }
    }
    Ok(MsgSubstream {
        name: name.to_string(),
        tag,
        dir_index,
        owner,
        span,
        len: bytes.len() as u64,
        text,
    })
}

/// Parse one recipient/attachment sub-storage.
#[allow(clippy::too_many_arguments)]
fn parse_storage(
    source: &[u8],
    cfbf: &crate::adapter::cfbf::CfbfModel,
    dir_index: u32,
    name: String,
    id: u32,
    e: &crate::adapter::cfbf::CfbfEntry,
    budget: &mut Budget,
    limits: Limits,
) -> Result<MsgStorage> {
    let prop_entry = find_child_stream(cfbf, dir_index, STREAM_PROPERTIES).ok_or_else(|| {
        invalid(format!(
            "the sub-storage {name} has no __properties_version1.0 stream"
        ))
    })?;
    let (prop_bytes, props_span) = stream_exact(source, prop_entry)?;
    let properties = parse_storage_properties(prop_bytes, props_span.0, dir_index, limits)?;
    budget.properties = budget.properties.saturating_add(properties.len() as u64);
    if budget.properties > limits.max_msg_properties {
        return Err(Error::resource_limit(format!(
            "the message has more than {} property entries",
            limits.max_msg_properties
        )));
    }
    let mut substreams: Vec<MsgSubstream> = Vec::new();
    for (ci, c) in cfbf.entries.iter().enumerate() {
        if !is_child_stream(c, dir_index) {
            continue;
        }
        let cname = crate::adapter::cfbf::entry_name_text(c);
        let Some(suffix) = cname.strip_prefix(SUBSTREAM_PREFIX) else {
            continue;
        };
        substreams.push(parse_substream(
            source,
            c,
            ci as u32,
            &cname,
            suffix,
            dir_index,
            &properties,
            budget,
            limits,
        )?);
    }
    Ok(MsgStorage {
        id,
        dir_index,
        span: e.entry_span,
        props_span,
        properties,
        substreams,
    })
}

/// Parse an `__substg1.0_<PPPPTTTT>` name suffix into its raw tag.
fn parse_tag_hex(suffix: &str) -> Result<u32> {
    let b = suffix.as_bytes();
    if b.len() != 8 || !b.iter().all(u8::is_ascii_hexdigit) {
        return Err(invalid(format!(
            "a substream name suffix {suffix:?} is not 8 PPPPTTTT hex digits"
        )));
    }
    let tag = u32::from_str_radix(suffix, 16)
        .map_err(|_| invalid("a substream name suffix is not valid hex"))?;
    Ok(tag)
}

/// Parse a sub-storage name suffix into its decimal id.
fn parse_decimal_id(suffix: &str, what: &str) -> Result<u32> {
    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid(format!(
            "a {what} storage name suffix {suffix:?} is not decimal digits"
        )));
    }
    suffix
        .parse::<u32>()
        .map_err(|_| invalid(format!("a {what} storage id is out of range")))
}

/// Decode UTF-16LE bytes; a trailing odd byte is dropped (a decoded-text caveat
/// only — the exact bytes remain available through the span).
fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
}

/// Map CP1252 bytes to their Unicode code points. The five CP1252-undefined
/// positions map to U+FFFD (a decoded-text caveat only; the exact bytes remain
/// available through the span).
fn decode_cp1252(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        let u = cp1252(b);
        // Every value this returns is a valid scalar value.
        out.push(char::from_u32(u as u32).unwrap_or('\u{FFFD}'));
    }
    out
}

/// Map one CP1252 byte to its Unicode code unit.
fn cp1252(b: u8) -> u16 {
    const HIGH: [u16; 32] = [
        0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, // 0x80..0x87
        0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0xFFFD, 0x017D, 0xFFFD, // 0x88..0x8F
        0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, // 0x90..0x97
        0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0x017E, 0x0178, // 0x98..0x9F
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as u16,
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn invalid(message: impl Into<String>) -> Error {
    Error::invalid_msg_structure(format!("msg: {}", message.into()))
}

fn corrupt(message: impl Into<String>) -> Error {
    Error::invalid_msg_structure(format!("msg model: {}", message.into()))
}

fn w_span(out: &mut Vec<u8>, s: (u64, u64)) {
    out.extend_from_slice(&s.0.to_le_bytes());
    out.extend_from_slice(&s.1.to_le_bytes());
}

fn w_props(out: &mut Vec<u8>, props: &[MsgProperty]) {
    out.extend_from_slice(&(props.len() as u32).to_le_bytes());
    for p in props {
        out.extend_from_slice(&p.tag.to_le_bytes());
        out.extend_from_slice(&p.flags.to_le_bytes());
        out.extend_from_slice(&p.value);
        w_span(out, p.span);
        out.extend_from_slice(&p.owner.to_le_bytes());
    }
}

fn w_subs(out: &mut Vec<u8>, subs: &[MsgSubstream]) {
    out.extend_from_slice(&(subs.len() as u32).to_le_bytes());
    for s in subs {
        out.extend_from_slice(&(s.name.len() as u32).to_le_bytes());
        out.extend_from_slice(s.name.as_bytes());
        out.extend_from_slice(&s.tag.to_le_bytes());
        out.extend_from_slice(&s.dir_index.to_le_bytes());
        out.extend_from_slice(&s.owner.to_le_bytes());
        w_span(out, s.span);
        out.extend_from_slice(&s.len.to_le_bytes());
        match &s.text {
            None => out.push(0),
            Some(t) => {
                out.push(1);
                out.extend_from_slice(&(t.len() as u32).to_le_bytes());
                out.extend_from_slice(t.as_bytes());
            }
        }
    }
}

fn w_storages(out: &mut Vec<u8>, sts: &[MsgStorage]) {
    out.extend_from_slice(&(sts.len() as u32).to_le_bytes());
    for s in sts {
        out.extend_from_slice(&s.id.to_le_bytes());
        out.extend_from_slice(&s.dir_index.to_le_bytes());
        w_span(out, s.span);
        w_span(out, s.props_span);
        w_props(out, &s.properties);
        w_subs(out, &s.substreams);
    }
}

fn r_props(r: &mut BinReader<'_>, doc_len: u64) -> Result<Vec<MsgProperty>> {
    let n = r.u32()?;
    if n > MAX_MODEL_PROPERTIES {
        return Err(corrupt("model property count is implausible"));
    }
    let mut out = Vec::new();
    for _ in 0..n {
        let tag = r.u32()?;
        let flags = r.u32()?;
        let mut value = [0u8; 8];
        value.copy_from_slice(r.bytes(8)?);
        let span = r.span()?;
        check_span(span, doc_len, "property span")?;
        if span.1 - span.0 != PROPERTY_ENTRY_LEN as u64 {
            return Err(corrupt("property span disagrees with its length"));
        }
        let owner = r.u32()?;
        out.push(MsgProperty {
            tag,
            flags,
            value,
            span,
            owner,
        });
    }
    Ok(out)
}

fn r_subs(r: &mut BinReader<'_>, doc_len: u64) -> Result<Vec<MsgSubstream>> {
    let n = r.u32()?;
    if n > MAX_MODEL_SUBSTREAMS {
        return Err(corrupt("model substream count is implausible"));
    }
    let mut out = Vec::new();
    let mut name_bytes: u64 = 0;
    for _ in 0..n {
        let name_len = r.u32()?;
        if name_len > MAX_MODEL_NAME_BYTES {
            return Err(corrupt("model name length is implausible"));
        }
        name_bytes = name_bytes.saturating_add(name_len as u64);
        let name_b = r.bytes(name_len as usize)?;
        let name = std::str::from_utf8(name_b)
            .map_err(|_| corrupt("model name is not UTF-8"))?
            .to_string();
        let tag = r.u32()?;
        let dir_index = r.u32()?;
        let owner = r.u32()?;
        let span = r.span()?;
        check_span(span, doc_len, "substream span")?;
        let len = r.u64()?;
        if span.1.saturating_sub(span.0) != len {
            return Err(corrupt("substream span disagrees with its length"));
        }
        let text = if r.u8()? != 0 {
            let tlen = r.u32()?;
            if tlen > MAX_MODEL_TEXT_UNITS {
                return Err(corrupt("model text length is implausible"));
            }
            let tb = r.bytes(tlen as usize)?;
            Some(
                std::str::from_utf8(tb)
                    .map_err(|_| corrupt("model text is not UTF-8"))?
                    .to_string(),
            )
        } else {
            None
        };
        out.push(MsgSubstream {
            name,
            tag,
            dir_index,
            owner,
            span,
            len,
            text,
        });
    }
    Ok(out)
}

fn r_storages(r: &mut BinReader<'_>, doc_len: u64) -> Result<Vec<MsgStorage>> {
    let n = r.u32()?;
    if n > MAX_MODEL_SUBSTORAGES {
        return Err(corrupt("model sub-storage count is implausible"));
    }
    let mut out = Vec::new();
    for _ in 0..n {
        let id = r.u32()?;
        let dir_index = r.u32()?;
        let span = r.span()?;
        check_span(span, doc_len, "sub-storage span")?;
        let props_span = r.span()?;
        check_span(props_span, doc_len, "sub-storage properties span")?;
        let properties = r_props(r, doc_len)?;
        let substreams = r_subs(r, doc_len)?;
        out.push(MsgStorage {
            id,
            dir_index,
            span,
            props_span,
            properties,
            substreams,
        });
    }
    Ok(out)
}

/// A minimal fail-closed little-endian reader for model decoding.
struct BinReader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> BinReader<'a> {
    fn new(b: &'a [u8]) -> Self {
        BinReader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("length overflow"))?;
        let out = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("unexpected end of model"))?;
        self.at = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn span(&mut self) -> Result<(u64, u64)> {
        Ok((self.u64()?, self.u64()?))
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_and_tag_names_are_stable() {
        assert_eq!(property_type_name(PT_UNICODE), "unicode");
        assert_eq!(property_type_name(PT_STRING8), "string8");
        assert_eq!(property_type_name(PT_BINARY), "binary");
        assert_eq!(property_type_name(0x0003), "long");
        assert_eq!(property_type_name(0x7777), "other");
        assert!(is_supported_type(PT_UNICODE));
        assert!(!is_supported_type(0x0003));
        assert_eq!(property_name(0x0037), "subject");
        assert_eq!(property_name(0x1000), "body");
        assert_eq!(property_name(0xDEAD), "unknown");
        let tag = 0x0037_001Fu32;
        assert_eq!(tag_id(tag), 0x0037);
        assert_eq!(tag_type(tag), 0x001F);
    }

    #[test]
    fn cp1252_maps_the_high_range() {
        assert_eq!(cp1252(0x41), 0x0041);
        assert_eq!(cp1252(0x80), 0x20AC); // euro sign
        assert_eq!(cp1252(0x99), 0x2122); // trade mark
        assert_eq!(cp1252(0x81), 0xFFFD); // undefined
        assert_eq!(decode_cp1252(b"Caf\xE9 \x80"), "Café €");
    }

    #[test]
    fn utf16le_decodes_units() {
        assert_eq!(decode_utf16le(&[0x48, 0x00, 0x69, 0x00]), "Hi");
        // An odd trailing byte is dropped.
        assert_eq!(decode_utf16le(&[0x48, 0x00, 0x69]), "H");
    }

    #[test]
    fn prose_is_not_a_msg() {
        assert!(!detect(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose.\n",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut seed: u64 = 0x2620_0004_4D53_4721;
        for _ in 0..1024 {
            let mut buf = [0u8; 1024];
            for b in buf.iter_mut() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            let _ = parse(&buf, Limits::STRICT);
        }
        let mut buf = vec![0u8; 4096];
        buf[..8].copy_from_slice(&crate::adapter::cfbf::SIGNATURE);
        for _ in 0..256 {
            for b in buf[8..].iter_mut() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            let _ = detect(&buf, Limits::STRICT);
            let _ = parse(&buf, Limits::STRICT);
        }
    }
}
