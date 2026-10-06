//! OPC model construction for the procedural field (Phase 12.3).
//!
//! This is the byte-facing half of the generic OPC layer: it byte-authoritatively
//! scans the exact package source (Phase 12.1), decodes **only** the OPC metadata
//! members (`[Content_Types].xml` and `*.rels`) with the existing bounded raw-DEFLATE
//! path (Phase 12.2), and hands the decoded parts to the pure
//! [`crate::adapter::package::opc`] parser. Non-metadata parts are never decoded
//! here; their exact bytes stay the 12.2 member raw spans.
//!
//! The result is the canonical `OpcModel` serialization stored in a
//! [`crate::field::node::NodeKind::PackageOpcModel`] seed node — derived (`Q_gen`)
//! state, materialized on demand and reused from the disposable cache.

use crate::adapter::package::opc;
use crate::adapter::package::zip::{ZipMember, scan};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// General-purpose flag bit 0: the member is encrypted.
const FLAG_ENCRYPTED: u16 = 0x0001;

/// Build the canonical serialization of the OPC model for an exact package source.
///
/// Fails closed with a typed error (`InvalidPackageStructure`/`InvalidXmlStructure`/
/// `InvalidZipStructure`/`ResourceLimit`) when the source is not a valid OPC package
/// or a metadata part is malformed; the exact bytes remain recoverable regardless.
pub fn build_opc_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let physical = scan(source, limits)?;
    physical.validate(source.len() as u64)?;

    let mut members: Vec<(u32, String)> = Vec::with_capacity(physical.members.len());
    for member in &physical.members {
        let name = core::str::from_utf8(&member.name).map_err(|_| {
            Error::invalid_package_structure("package member name is not valid UTF-8")
        })?;
        let part = opc::part_name_from_member(name)?;
        members.push((member.id.ordinal, part));
    }

    let mut content_types: Option<Vec<u8>> = None;
    let mut rels: Vec<(Option<u32>, Vec<u8>)> = Vec::new();
    let mut issues: Vec<String> = Vec::new();

    for member in &physical.members {
        let Ok(raw_name) = core::str::from_utf8(&member.name) else {
            continue;
        };
        let is_content_types = raw_name.eq_ignore_ascii_case("[Content_Types].xml");
        let is_package_rels = raw_name.eq_ignore_ascii_case("_rels/.rels");
        let owner_member = if is_content_types || is_package_rels {
            None
        } else {
            part_rels_owner_member(raw_name)
        };
        if !is_content_types && !is_package_rels && owner_member.is_none() {
            continue;
        }

        let Some(bytes) = decode_metadata_member(source, member, limits) else {
            issues.push(format!(
                "opc: metadata member {raw_name:?} could not be decoded"
            ));
            continue;
        };

        if is_content_types {
            content_types = Some(bytes);
        } else if is_package_rels {
            rels.push((None, bytes));
        } else if let Some(owner) = owner_member {
            let owner_part = format!("/{owner}");
            match members
                .iter()
                .find(|(_, n)| n.eq_ignore_ascii_case(&owner_part))
            {
                Some((ordinal, _)) => rels.push((Some(*ordinal), bytes)),
                None => issues.push(format!(
                    "opc: relationships member {raw_name:?} has no owner part"
                )),
            }
        }
    }

    let rel_views: Vec<(Option<u32>, &[u8])> = rels
        .iter()
        .map(|(owner, bytes)| (*owner, bytes.as_slice()))
        .collect();
    let mut model = opc::parse_opc(&members, content_types.as_deref(), &rel_views, limits)?;
    model.issues.extend(issues);
    Ok(model.encode())
}

/// For a member name `<prefix>/_rels/<file>.rels`, the owning member name
/// `<prefix>/<file>`; `None` when the member is not a part-relationships part.
fn part_rels_owner_member(name: &str) -> Option<String> {
    let marker = "_rels/";
    let idx = name.rfind(marker)?;
    if idx != 0 && name.as_bytes()[idx - 1] != b'/' {
        return None;
    }
    let file = &name[idx + marker.len()..];
    let owner_file = file.strip_suffix(".rels")?;
    if owner_file.is_empty() {
        return None;
    }
    Some(format!("{}{owner_file}", &name[..idx]))
}

/// Decode one member to bytes when it is small enough and decodable (stored or
/// raw-DEFLATE, not encrypted). Never allocates beyond `max_xml_part_bytes`.
fn decode_metadata_member(source: &[u8], member: &ZipMember, limits: Limits) -> Option<Vec<u8>> {
    if member.flags & FLAG_ENCRYPTED != 0 {
        return None;
    }
    if member.uncompressed_size > limits.max_xml_part_bytes {
        return None;
    }
    let off = usize::try_from(member.data.0).ok()?;
    let len = usize::try_from(member.data.1).ok()?;
    let end = off.checked_add(len)?;
    let raw = source.get(off..end)?;
    match member.method {
        0 => Some(raw.to_vec()),
        8 => crate::field::derive::inflate_raw_deflate(raw, member.uncompressed_size, limits).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rels_owner_member_recognition() {
        assert_eq!(
            part_rels_owner_member("word/_rels/document.xml.rels").as_deref(),
            Some("word/document.xml")
        );
        assert_eq!(
            part_rels_owner_member("_rels/foo.rels").as_deref(),
            Some("foo")
        );
        assert_eq!(part_rels_owner_member("_rels/.rels"), None);
        assert_eq!(part_rels_owner_member("word/document.xml"), None);
        assert_eq!(part_rels_owner_member("a_rels/b.rels"), None);
    }
}
