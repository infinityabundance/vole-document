#![cfg(feature = "field")]
//! Phase 16.1: the shipped DEFLATE inflate backend is `zlib-rs`.
//!
//! These are **differential** witnesses. The streams are produced by an
//! *independent* zlib implementation (`flate2`'s `zlib-rs` backend, a
//! dev-dependency) and by the real `real100-v1` corpus members; the shipped
//! backend (`zlib-rs`, reached through `derive::inflate_zlib` /
//! `derive::inflate_raw_deflate`) must reproduce, byte-for-byte, both the source
//! bytes and the retained `miniz_oxide` reference decode.
//!
//! A backend change that altered any decoded byte would fail here (and in
//! `tools/phase16-zlib-endtoend-court.sh`), so exactness is not assumed.

use std::io::Write;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::{DeflateEncoder, ZlibEncoder};
use vole_document::Limits;
use vole_document::field::derive::{inflate_raw_deflate, inflate_zlib};

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn zlib_encode(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).expect("zlib encode");
    e.finish().expect("zlib finish")
}

fn raw_deflate_encode(data: &[u8]) -> Vec<u8> {
    let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).expect("deflate encode");
    e.finish().expect("deflate finish")
}

/// Empty, tiny, highly compressible, repetitive, and high-entropy inputs.
fn battery() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"hello, world".to_vec(),
        vec![0u8; 200_000],
        b"abcdefghijklmnop".repeat(20_000),
        (0..=255u8).cycle().take(100_000).collect(),
    ];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..4 {
        let n = (xorshift64(&mut state) % 262_144) as usize;
        let mut buf = Vec::with_capacity(n);
        for _ in 0..n {
            buf.push((xorshift64(&mut state) & 0xFF) as u8);
        }
        v.push(buf);
    }
    v
}

#[test]
fn zlib_backend_matches_an_independent_encoder_and_miniz() {
    for (i, data) in battery().into_iter().enumerate() {
        let encoded = zlib_encode(&data);
        let got = inflate_zlib(&encoded, data.len() as u64, Limits::DEFAULT)
            .unwrap_or_else(|e| panic!("case {i}: shipped zlib decode failed: {e}"));
        let reference = miniz_oxide::inflate::decompress_to_vec_zlib(&encoded).unwrap();
        assert_eq!(
            got, reference,
            "case {i}: differs from the miniz_oxide reference"
        );
        assert_eq!(got, data, "case {i}: must reproduce the source bytes");
    }
}

#[test]
fn raw_backend_matches_an_independent_encoder_and_miniz() {
    for (i, data) in battery().into_iter().enumerate() {
        let encoded = raw_deflate_encode(&data);
        let got = inflate_raw_deflate(&encoded, data.len() as u64, Limits::DEFAULT)
            .unwrap_or_else(|e| panic!("case {i}: shipped raw decode failed: {e}"));
        let reference =
            miniz_oxide::inflate::decompress_to_vec_with_limit(&encoded, data.len()).unwrap();
        assert_eq!(
            got, reference,
            "case {i}: differs from the miniz_oxide reference"
        );
        assert_eq!(got, data, "case {i}: must reproduce the source bytes");
    }
}

#[test]
fn a_wrong_declared_length_is_a_typed_error() {
    let data = b"exact length".repeat(1000);
    let z = zlib_encode(&data);
    assert!(inflate_zlib(&z, data.len() as u64 + 1, Limits::DEFAULT).is_err());
    assert!(inflate_zlib(&z, data.len() as u64 - 1, Limits::DEFAULT).is_err());
    // A zlib-wrapped stream offered as raw DEFLATE must decline.
    assert!(inflate_raw_deflate(&z, data.len() as u64, Limits::DEFAULT).is_err());
}

// ---------------------------------------------------------------------------
// Real-corpus differential (bounded; requires the `package` scanners)
// ---------------------------------------------------------------------------

#[cfg(feature = "package")]
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect_files(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

/// Byte-identity against `miniz_oxide` on real `real100-v1` compressed members.
///
/// Bounded: skips files above 48 MiB, stops after 24 documents or 2000 members,
/// and caps any single reference decode at 64 MiB, so the test is cheap while
/// still exercising real zlib (PDF) and real raw-DEFLATE (ZIP) streams. It is a
/// no-op when the corpus is absent.
#[cfg(feature = "package")]
#[test]
fn backend_matches_miniz_on_real_corpus_members() {
    use vole_document::adapter::package::zip;
    use vole_document::adapter::pdf::{FilterClass, physical};

    const REF_CAP: usize = 64 * 1024 * 1024;
    let root = PathBuf::from("real100-v1/documents");
    if !root.is_dir() {
        eprintln!("real100-v1/documents not present; skipping the real-corpus differential");
        return;
    }
    let mut files = Vec::new();
    collect_files(&root, &mut files);
    files.sort();

    let (mut docs, mut members) = (0usize, 0usize);
    for path in files {
        if docs >= 24 || members >= 2000 {
            break;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.len() > 48 * 1024 * 1024 {
            continue;
        }
        docs += 1;

        if let Ok(scan) = physical::scan(&bytes, Limits::DEFAULT) {
            for s in &scan.streams {
                if s.filter != FilterClass::FlateDecode {
                    continue;
                }
                let (Ok(a), Ok(b)) = (usize::try_from(s.data_start), usize::try_from(s.data_len))
                else {
                    continue;
                };
                let Some(payload) = bytes.get(a..a.saturating_add(b)) else {
                    continue;
                };
                let Ok(reference) =
                    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(payload, REF_CAP)
                else {
                    continue;
                };
                let got = inflate_zlib(payload, reference.len() as u64, Limits::DEFAULT)
                    .expect("shipped zlib decode");
                assert_eq!(got, reference, "pdf member in {}", path.display());
                members += 1;
            }
        }

        if let Ok(z) = zip::scan(&bytes, Limits::DEFAULT) {
            for m in &z.members {
                if m.method != 8 || m.uncompressed_size as usize > REF_CAP {
                    continue;
                }
                let Ok(raw) = z.member_raw(m, &bytes) else {
                    continue;
                };
                let Ok(reference) = miniz_oxide::inflate::decompress_to_vec_with_limit(
                    raw,
                    m.uncompressed_size as usize,
                ) else {
                    continue;
                };
                let got = inflate_raw_deflate(raw, m.uncompressed_size, Limits::DEFAULT)
                    .expect("shipped raw decode");
                assert_eq!(got, reference, "zip member in {}", path.display());
                members += 1;
            }
        }
    }
    eprintln!("real-corpus differential: {docs} docs, {members} members");
    assert!(members > 0, "no real corpus members were compared");
}
