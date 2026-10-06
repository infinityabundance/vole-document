//! Phase 12.1 court: the byte-authoritative ZIP physical layer.
//!
//! Every fixture here is built by a tiny, dependency-free writer in this file, so
//! the ground truth (member order, offsets, sizes, descriptor presence, ZIP64
//! layout) is known exactly. The court asserts the predeclared Phase-12.1 gates:
//!
//! 1. Cover — the span partition is exactly `[0, len)`, with every byte classed.
//! 2. Exactness — the cover re-emits the source byte-for-byte and `member_raw`
//!    returns the member's exact compressed span.
//! 3. Identity — physical identity is `(ordinal, local-header offset)`; duplicate
//!    names produce distinct identities and are never collapsed.
//! 4. Hostile-safe — malformed or contradictory archives are typed errors or
//!    exact-preserving declines, never a panic, and the cover still validates for
//!    the opaque-preserving cases.
//! 5. Determinism — scanning twice yields identical structures.
//!
//! Run under the `package` feature (`cargo test --all-features`).
#![cfg(feature = "package")]

use vole_document::adapter::package::{
    DescriptorStyle, NameHazard, ZipSpanKind, classify_name, crc32_iso_hdlc, require_safe_name,
    scan, verify_stored_crc,
};
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;

// ---------------------------------------------------------------------------
// Dependency-free ZIP writer (test ground truth)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Entry {
    name: Vec<u8>,
    /// Logical content (drives CRC and uncompressed size).
    content: Vec<u8>,
    /// Bytes physically written as the member payload (raw DEFLATE when method 8).
    compressed: Vec<u8>,
    method: u16,
    /// Extra general-purpose flag bits (bit 11 UTF-8 is set automatically).
    flags_extra: u16,
    /// `Some(signed)` writes a data descriptor after the payload.
    descriptor: Option<bool>,
    comment: Vec<u8>,
    /// Emit sentinel sizes plus the ZIP64 extended-information extra field.
    zip64: bool,
    local_extra: Vec<u8>,
    central_extra: Vec<u8>,
    /// Override the central entry's declared uncompressed size (hostile fixture).
    central_uncompressed_override: Option<u32>,
    /// Override the central entry's declared local-header offset (hostile fixture).
    lfh_offset_override: Option<u32>,
}

#[derive(Debug, Clone, Default)]
struct Archive {
    prefix: Vec<u8>,
    entries: Vec<Entry>,
    comment: Vec<u8>,
    trailing: Vec<u8>,
    zip64_eocd: bool,
}

impl Entry {
    fn stored(name: &[u8], content: &[u8]) -> Entry {
        Entry {
            name: name.to_vec(),
            content: content.to_vec(),
            compressed: content.to_vec(),
            method: 0,
            ..Entry::default()
        }
    }

    fn deflated(name: &[u8], content: &[u8]) -> Entry {
        Entry {
            name: name.to_vec(),
            content: content.to_vec(),
            compressed: raw_stored_deflate(content),
            method: 8,
            ..Entry::default()
        }
    }
}

/// A valid single-block raw DEFLATE stream that stores `data` uncompressed.
fn raw_stored_deflate(data: &[u8]) -> Vec<u8> {
    assert!(
        data.len() <= 0xFFFF,
        "test deflate helper is single-block only"
    );
    let mut out = Vec::with_capacity(data.len() + 5);
    out.push(0x01); // BFINAL=1, BTYPE=00 (stored)
    let len = data.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn build_zip(a: &Archive) -> Vec<u8> {
    let mut out = a.prefix.clone();
    let mut offsets: Vec<u32> = Vec::new();
    let mut crcs: Vec<u32> = Vec::new();

    for e in &a.entries {
        offsets.push(out.len() as u32);
        crcs.push(crc32_iso_hdlc(&e.content));

        let mut flags = e.flags_extra;
        if e.descriptor.is_some() {
            flags |= 0x0008;
        }
        if e.name.iter().any(|&b| b >= 0x80) {
            flags |= 0x0800;
        }

        let mut local_extra = e.local_extra.clone();
        if e.zip64 {
            put_u16(&mut local_extra, 0x0001);
            put_u16(&mut local_extra, 16);
            put_u64(&mut local_extra, e.content.len() as u64);
            put_u64(&mut local_extra, e.compressed.len() as u64);
        }

        let (lcsize, lusize) = if e.zip64 {
            (0xFFFF_FFFFu32, 0xFFFF_FFFFu32)
        } else if e.descriptor.is_some() {
            (0, 0)
        } else {
            (e.compressed.len() as u32, e.content.len() as u32)
        };

        put_u32(&mut out, 0x0403_4b50); // local file header
        put_u16(&mut out, if e.zip64 { 45 } else { 20 }); // version needed
        put_u16(&mut out, flags);
        put_u16(&mut out, e.method);
        put_u16(&mut out, 0); // MS-DOS time
        put_u16(&mut out, 0x0021); // MS-DOS date
        put_u32(&mut out, crcs[offsets.len() - 1]);
        put_u32(&mut out, lcsize);
        put_u32(&mut out, lusize);
        put_u16(&mut out, e.name.len() as u16);
        put_u16(&mut out, local_extra.len() as u16);
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&local_extra);
        out.extend_from_slice(&e.compressed);

        if let Some(signed) = e.descriptor {
            if signed {
                put_u32(&mut out, 0x0807_4b50);
            }
            put_u32(&mut out, crcs[offsets.len() - 1]);
            put_u32(&mut out, e.compressed.len() as u32);
            put_u32(&mut out, e.content.len() as u32);
        }
    }

    let cd_offset = out.len() as u32;
    for (i, e) in a.entries.iter().enumerate() {
        let mut central_extra = e.central_extra.clone();
        if e.zip64 {
            put_u16(&mut central_extra, 0x0001);
            put_u16(&mut central_extra, 24);
            put_u64(&mut central_extra, e.content.len() as u64);
            put_u64(&mut central_extra, e.compressed.len() as u64);
            put_u64(&mut central_extra, u64::from(offsets[i]));
        }
        let (ccsize, cusize, coff) = if e.zip64 {
            (
                0xFFFF_FFFFu32,
                0xFFFF_FFFFu32,
                e.lfh_offset_override.unwrap_or(0xFFFF_FFFF),
            )
        } else {
            (
                e.compressed.len() as u32,
                e.central_uncompressed_override
                    .unwrap_or(e.content.len() as u32),
                e.lfh_offset_override.unwrap_or(offsets[i]),
            )
        };

        put_u32(&mut out, 0x0201_4b50); // central file header
        put_u16(&mut out, 45); // version made by
        put_u16(&mut out, if e.zip64 { 45 } else { 20 }); // version needed
        put_u16(&mut out, {
            let mut f = e.flags_extra;
            if e.descriptor.is_some() {
                f |= 0x0008;
            }
            if e.name.iter().any(|&b| b >= 0x80) {
                f |= 0x0800;
            }
            f
        });
        put_u16(&mut out, e.method);
        put_u16(&mut out, 0); // time
        put_u16(&mut out, 0x0021); // date
        put_u32(&mut out, crcs[i]);
        put_u32(&mut out, ccsize);
        put_u32(&mut out, cusize);
        put_u16(&mut out, e.name.len() as u16);
        put_u16(&mut out, central_extra.len() as u16);
        put_u16(&mut out, e.comment.len() as u16);
        put_u16(&mut out, 0); // disk start
        put_u16(&mut out, 0); // internal attrs
        put_u32(&mut out, 0); // external attrs
        put_u32(&mut out, coff);
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&central_extra);
        out.extend_from_slice(&e.comment);
    }
    let cd_size = out.len() as u32 - cd_offset;
    let count = a.entries.len() as u16;

    if a.zip64_eocd {
        let eocd64_off = out.len() as u64;
        put_u32(&mut out, 0x0606_4b50);
        put_u64(&mut out, 44); // record size
        put_u16(&mut out, 45);
        put_u16(&mut out, 45);
        put_u32(&mut out, 0); // disk
        put_u32(&mut out, 0); // cd start disk
        put_u64(&mut out, u64::from(count));
        put_u64(&mut out, u64::from(count));
        put_u64(&mut out, u64::from(cd_size));
        put_u64(&mut out, u64::from(cd_offset));
        put_u32(&mut out, 0x0706_4b50); // locator
        put_u32(&mut out, 0);
        put_u64(&mut out, eocd64_off);
        put_u32(&mut out, 1); // total disks
        put_u32(&mut out, 0x0605_4b50); // EOCD with sentinels
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0xFFFF);
        put_u16(&mut out, 0xFFFF);
        put_u32(&mut out, 0xFFFF_FFFF);
        put_u32(&mut out, 0xFFFF_FFFF);
    } else {
        put_u32(&mut out, 0x0605_4b50);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, count);
        put_u16(&mut out, count);
        put_u32(&mut out, cd_size);
        put_u32(&mut out, cd_offset);
    }
    put_u16(&mut out, a.comment.len() as u16);
    out.extend_from_slice(&a.comment);
    out.extend_from_slice(&a.trailing);
    out
}

/// Offset of the classic EOCD record in a built archive.
fn eocd_offset(bytes: &[u8]) -> usize {
    bytes.len()
        - 22
        - usize::from(u16::from_le_bytes([
            bytes[bytes.len() - 2],
            bytes[bytes.len() - 1],
        ]))
}

fn assert_cover_is_exact(zip: &vole_document::adapter::package::ZipPhysical, bytes: &[u8]) {
    zip.validate(bytes.len() as u64)
        .expect("cover must validate");
    zip.reemits(bytes).expect("cover must re-emit the source");
    assert_eq!(zip.total_len(), bytes.len() as u64);
}

// ---------------------------------------------------------------------------
// Golden fixtures
// ---------------------------------------------------------------------------

#[test]
fn golden_stored_and_deflated() {
    let a = Archive {
        entries: vec![
            Entry::stored(b"hello.txt", b"hello world"),
            Entry::deflated(b"nested/page.xml", b"<a>content</a>"),
        ],
        ..Archive::default()
    };
    let bytes = build_zip(&a);

    let zip = scan(&bytes, Limits::DEFAULT).expect("golden scan");
    assert_cover_is_exact(&zip, &bytes);
    assert_eq!(zip.members.len(), 2);
    assert_eq!(zip.central.len(), 2);
    assert!(zip.zip64.is_none());

    let m0 = &zip.members[0];
    assert_eq!(m0.name, b"hello.txt");
    assert_eq!(m0.method, 0);
    assert_eq!(m0.id.ordinal, 0);
    assert_eq!(m0.id.local_header_offset, 0);
    assert_eq!(m0.local_header, (0, 30 + 9));
    assert_eq!(zip.member_raw(m0, &bytes).unwrap(), b"hello world");
    verify_stored_crc(m0, zip.member_raw(m0, &bytes).unwrap()).expect("stored CRC verifies");

    let m1 = &zip.members[1];
    assert_eq!(m1.name, b"nested/page.xml");
    assert_eq!(m1.method, 8);
    assert_eq!(
        zip.member_raw(m1, &bytes).unwrap(),
        raw_stored_deflate(b"<a>content</a>").as_slice()
    );

    // Every structural class is present and no byte is Unclassified here.
    assert!(zip.spans.iter().any(|s| s.kind == ZipSpanKind::LocalHeader));
    assert!(zip.spans.iter().any(|s| s.kind == ZipSpanKind::MemberData));
    assert!(
        zip.spans
            .iter()
            .any(|s| s.kind == ZipSpanKind::CentralDirectory)
    );
    assert!(zip.spans.iter().any(|s| s.kind == ZipSpanKind::Eocd));
    assert!(
        !zip.spans
            .iter()
            .any(|s| s.kind == ZipSpanKind::Unclassified)
    );
    assert_eq!(zip.spans[0].start, 0);
}

#[test]
fn data_descriptor_with_and_without_signature() {
    let mut signed = Entry::stored(b"signed.bin", b"aaaaaaaa");
    signed.descriptor = Some(true);
    let mut unsigned = Entry::stored(b"unsigned.bin", b"bbbbbbbb");
    unsigned.descriptor = Some(false);
    let a = Archive {
        entries: vec![signed, unsigned],
        ..Archive::default()
    };
    let bytes = build_zip(&a);

    let zip = scan(&bytes, Limits::DEFAULT).expect("descriptor scan");
    assert_cover_is_exact(&zip, &bytes);

    let d0 = zip.members[0].data_descriptor.expect("signed descriptor");
    assert_eq!(d0.style, DescriptorStyle::Signature);
    assert_eq!(d0.span.1, 16);
    let d1 = zip.members[1].data_descriptor.expect("unsigned descriptor");
    assert_eq!(d1.style, DescriptorStyle::NoSignature);
    assert_eq!(d1.span.1, 12);

    assert!(
        zip.spans
            .iter()
            .any(|s| s.kind == ZipSpanKind::DataDescriptor(DescriptorStyle::Signature))
    );
    assert!(
        zip.spans
            .iter()
            .any(|s| s.kind == ZipSpanKind::DataDescriptor(DescriptorStyle::NoSignature))
    );
}

#[test]
fn zip64_eocd_locator_and_member_extra() {
    let mut e = Entry::stored(b"big.bin", b"zip64 member payload");
    e.zip64 = true;
    let a = Archive {
        entries: vec![e],
        zip64_eocd: true,
        ..Archive::default()
    };
    let bytes = build_zip(&a);

    let zip = scan(&bytes, Limits::DEFAULT).expect("zip64 scan");
    assert_cover_is_exact(&zip, &bytes);
    let z = zip.zip64.as_ref().expect("zip64 records");
    assert_eq!(z.eocd.total_entries, 1);
    assert_eq!(z.eocd.size_of_record, 44);
    assert_eq!(z.locator.total_disks, 1);
    assert!(zip.spans.iter().any(|s| s.kind == ZipSpanKind::Zip64Eocd));
    assert!(
        zip.spans
            .iter()
            .any(|s| s.kind == ZipSpanKind::Zip64EocdLocator)
    );

    let m = &zip.members[0];
    assert_eq!(m.compressed_size, 20);
    assert_eq!(m.uncompressed_size, 20);
    assert_eq!(zip.member_raw(m, &bytes).unwrap(), b"zip64 member payload");
}

#[test]
fn prefix_archive_comment_entry_comment_and_trailing() {
    let mut e = Entry::stored(b"a.txt", b"payload");
    e.comment = b"entry note".to_vec();
    let a = Archive {
        prefix: b"SFX-STUB".to_vec(),
        entries: vec![e],
        comment: b"archive comment".to_vec(),
        trailing: b"TRAIL".to_vec(),
        ..Archive::default()
    };
    let bytes = build_zip(&a);

    let zip = scan(&bytes, Limits::DEFAULT).expect("prefix/trailing scan");
    assert_cover_is_exact(&zip, &bytes);
    assert_eq!(zip.prefix, Some((0, 8)));
    assert_eq!(zip.trailing, Some((bytes.len() as u64 - 5, 5)));
    assert_eq!(zip.archive_comment.1, 15);
    assert_eq!(zip.members[0].comment.1, 10);

    assert_eq!(zip.spans[0].kind, ZipSpanKind::Prefix);
    assert_eq!(zip.spans.last().unwrap().kind, ZipSpanKind::Trailing);
    // The member's local header offset accounts for the prefix.
    assert_eq!(zip.members[0].local_header_offset, 8);
    assert_eq!(zip.member_raw(&zip.members[0], &bytes).unwrap(), b"payload");
}

#[test]
fn duplicate_names_are_distinct_physical_identities() {
    let a = Archive {
        entries: vec![
            Entry::stored(b"dup.txt", b"first"),
            Entry::stored(b"dup.txt", b"second"),
        ],
        ..Archive::default()
    };
    let bytes = build_zip(&a);

    let zip = scan(&bytes, Limits::DEFAULT).expect("duplicate-name scan");
    assert_cover_is_exact(&zip, &bytes);
    assert_eq!(zip.members.len(), 2);
    assert_eq!(zip.members[0].name, zip.members[1].name);
    assert_ne!(zip.members[0].id, zip.members[1].id);
    assert_eq!(zip.members[0].id.ordinal, 0);
    assert_eq!(zip.members[1].id.ordinal, 1);
    assert_eq!(zip.member_raw(&zip.members[0], &bytes).unwrap(), b"first");
    assert_eq!(zip.member_raw(&zip.members[1], &bytes).unwrap(), b"second");
}

#[test]
fn non_ascii_name_bytes_are_preserved_verbatim() {
    let name = b"caf\xc3\xa9.xml";
    let a = Archive {
        entries: vec![Entry::stored(name, b"x")],
        ..Archive::default()
    };
    let bytes = build_zip(&a);
    let zip = scan(&bytes, Limits::DEFAULT).expect("utf8 flag scan");
    assert_cover_is_exact(&zip, &bytes);
    assert_eq!(zip.members[0].name, name);
    assert_ne!(zip.members[0].flags & 0x0800, 0, "UTF-8 flag preserved");
}

// ---------------------------------------------------------------------------
// Hostile fixtures
// ---------------------------------------------------------------------------

fn assert_typed_err(err: vole_document::Error) {
    assert!(
        matches!(
            err.class(),
            ErrorClass::InvalidZipStructure
                | ErrorClass::CoverageViolation
                | ErrorClass::ResourceLimit
                | ErrorClass::UnsupportedFeature
        ),
        "unexpected error class {:?}: {}",
        err.class(),
        err.message()
    );
}

#[test]
fn truncated_archive_is_typed_error() {
    let a = Archive {
        entries: vec![Entry::stored(b"a.txt", b"hello world")],
        ..Archive::default()
    };
    let bytes = build_zip(&a);
    for cut in [1usize, 4, 8, bytes.len() / 2, bytes.len() - 1] {
        match scan(&bytes[..cut], Limits::STRICT) {
            Ok(z) => assert_cover_is_exact(&z, &bytes[..cut]),
            Err(e) => assert_typed_err(e),
        }
    }
}

#[test]
fn bad_eocd_signature_is_typed_error() {
    let a = Archive {
        entries: vec![Entry::stored(b"a.txt", b"hello")],
        ..Archive::default()
    };
    let mut bytes = build_zip(&a);
    let off = eocd_offset(&bytes);
    bytes[off] ^= 0xFF;
    assert_typed_err(scan(&bytes, Limits::DEFAULT).unwrap_err());
}

#[test]
fn bad_central_offset_is_typed_error() {
    let a = Archive {
        entries: vec![Entry::stored(b"a.txt", b"hello")],
        ..Archive::default()
    };
    let mut bytes = build_zip(&a);
    let off = eocd_offset(&bytes);
    // Point the central directory at the local header region (garbage for CD).
    bytes[off + 16..off + 20].copy_from_slice(&0u32.to_le_bytes());
    assert_typed_err(scan(&bytes, Limits::DEFAULT).unwrap_err());
}

#[test]
fn overlapping_spans_are_rejected() {
    let mut second = Entry::stored(b"dup.bin", b"samebytes");
    second.lfh_offset_override = Some(0); // alias the first member's local header
    let a = Archive {
        entries: vec![Entry::stored(b"dup.bin", b"samebytes"), second],
        ..Archive::default()
    };
    let bytes = build_zip(&a);
    assert_typed_err(scan(&bytes, Limits::DEFAULT).unwrap_err());
}

#[test]
fn impossible_offset_is_rejected() {
    let mut e = Entry::stored(b"a.bin", b"bytes");
    e.lfh_offset_override = Some(0xFFFF_FFF0);
    let a = Archive {
        entries: vec![e],
        ..Archive::default()
    };
    let bytes = build_zip(&a);
    assert_typed_err(scan(&bytes, Limits::DEFAULT).unwrap_err());
}

#[test]
fn zip64_sentinel_without_locator_is_rejected() {
    let a = Archive {
        entries: vec![Entry::stored(b"a.bin", b"bytes")],
        ..Archive::default()
    };
    let mut bytes = build_zip(&a);
    let off = eocd_offset(&bytes);
    bytes[off + 10..off + 12].copy_from_slice(&0xFFFFu16.to_le_bytes());
    bytes[off + 12..off + 16].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    bytes[off + 16..off + 20].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    let err = scan(&bytes, Limits::DEFAULT).unwrap_err();
    assert_eq!(err.class(), ErrorClass::InvalidZipStructure);
}

#[test]
fn traversal_name_scans_but_is_declined_at_the_boundary() {
    let a = Archive {
        entries: vec![Entry::stored(b"../etc/passwd", b"secrets")],
        ..Archive::default()
    };
    let bytes = build_zip(&a);
    // The physical cover is sound: a hostile name never breaks it.
    let zip = scan(&bytes, Limits::DEFAULT).expect("traversal scan preserves the cover");
    assert_cover_is_exact(&zip, &bytes);
    assert_eq!(zip.members[0].name, b"../etc/passwd");
    assert_eq!(
        classify_name(&zip.members[0].name),
        Some(NameHazard::Traversal)
    );
    assert_eq!(
        require_safe_name(&zip.members[0].name).unwrap_err().class(),
        ErrorClass::InvalidZipStructure
    );
}

#[test]
fn huge_declared_size_is_a_resource_limit() {
    let mut e = Entry::stored(b"bomb.bin", b"tiny");
    e.central_uncompressed_override = Some(1 << 30);
    let a = Archive {
        entries: vec![e],
        ..Archive::default()
    };
    let bytes = build_zip(&a);
    let err = scan(&bytes, Limits::STRICT).unwrap_err();
    assert_eq!(err.class(), ErrorClass::ResourceLimit);
}

#[test]
fn member_count_limit_is_enforced() {
    let a = Archive {
        entries: vec![Entry::stored(b"a", b"1"), Entry::stored(b"b", b"2")],
        ..Archive::default()
    };
    let bytes = build_zip(&a);
    let limits = Limits {
        max_zip_members: 1,
        ..Limits::STRICT
    };
    let err = scan(&bytes, limits).unwrap_err();
    assert_eq!(err.class(), ErrorClass::ResourceLimit);
}

#[test]
fn stored_crc_corruption_is_detected() {
    let a = Archive {
        entries: vec![Entry::stored(b"a.bin", b"correct bytes")],
        ..Archive::default()
    };
    let bytes = build_zip(&a);
    let zip = scan(&bytes, Limits::DEFAULT).expect("scan");
    let m = &zip.members[0];
    verify_stored_crc(m, zip.member_raw(m, &bytes).unwrap()).expect("clean CRC verifies");
    assert_eq!(
        verify_stored_crc(m, b"wrong bytes!!").unwrap_err().class(),
        ErrorClass::IntegrityMismatch
    );
}

// ---------------------------------------------------------------------------
// Determinism and a bounded no-panic property
// ---------------------------------------------------------------------------

#[test]
fn scanning_is_deterministic() {
    let mut e = Entry::deflated(b"doc.xml", b"<x>hello</x>");
    e.comment = b"c".to_vec();
    let a = Archive {
        prefix: b"PFX".to_vec(),
        entries: vec![Entry::stored(b"mimetype", b"application/epub+zip"), e],
        comment: b"note".to_vec(),
        trailing: b"TR".to_vec(),
        ..Archive::default()
    };
    let bytes = build_zip(&a);
    let first = scan(&bytes, Limits::DEFAULT).expect("first scan");
    let second = scan(&bytes, Limits::DEFAULT).expect("second scan");
    assert_eq!(first, second, "scan must be deterministic");
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(n + 8);
        while v.len() < n {
            v.extend_from_slice(&self.next_u64().to_le_bytes());
        }
        v.truncate(n);
        v
    }
}

#[test]
fn random_bytes_never_panic_and_keep_the_cover() {
    let iters: usize = std::env::var("VOLE_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(3000);
    let mut rng = Rng::new(0x5EED_1201);
    for i in 0..iters {
        let len = (rng.next_u64() % 4097) as usize;
        let bytes = rng.bytes(len);
        match scan(&bytes, Limits::STRICT) {
            Ok(zip) => {
                assert_cover_is_exact(&zip, &bytes);
                // Scanning twice on accepted input is stable.
                let again = scan(&bytes, Limits::STRICT).expect("deterministic accept");
                assert_eq!(zip, again, "iteration {i}");
            }
            Err(e) => assert_ne!(
                e.class(),
                ErrorClass::InternalInvariant,
                "iteration {i}: hostile input reported an internal invariant"
            ),
        }
    }
}
