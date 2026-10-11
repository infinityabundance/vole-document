//! Phase 21.34 court fixture generator.
//!
//! Writes a small, deterministic set of NetCDF classic (CDF-1/CDF-2) and
//! HDF5-subset fixtures into a directory, so the scientific-container court needs
//! neither python3, h5py, netCDF, nor `h5dump` — only the shipped toolchain. The
//! generator is dependency-free: it writes the structures byte-for-byte, so the
//! ground truth is known exactly.
//!
//! ```sh
//! cargo run --locked --all-features --example hdf_fixtures -- OUTDIR
//! ```
//!
//! The HDF5 writer emits a documented subset: a **v0 superblock** (8-byte offsets
//! and lengths), a root group reached through a **Symbol Table message** + a
//! type-0 **B-tree** + a **local heap**, **v1 object headers** whose message list
//! carries the **dataspace** / **datatype** / **contiguous data layout** / **v1
//! attribute** messages, and a nested group. The controls add a **filter-pipeline**
//! message (typed decline), a **v2 object header** byte (typed decline), a truncated
//! superblock (not detected), and a bad-version superblock (not detected).

#![cfg(feature = "hdf")]
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

fn le16(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}
fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}
fn le64(v: u64) -> [u8; 8] {
    v.to_le_bytes()
}

const HDF5_SIG: [u8; 8] = [0x89, b'H', b'D', b'F', b'\r', b'\n', 0x1a, b'\n'];
const UNDEF: u64 = 0xffff_ffff_ffff_ffff;

// ---------------------------------------------------------------------------
// NetCDF classic writer (CDF-1 = 4-byte offsets, CDF-2 = 8-byte offsets)
// ---------------------------------------------------------------------------

fn nc_push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// A `NON_NEG`: 32-bit in CDF-1, 64-bit in CDF-2.
fn nc_push_nonneg(out: &mut Vec<u8>, off: usize, v: u64) {
    if off == 8 {
        out.extend_from_slice(&v.to_be_bytes());
    } else {
        out.extend_from_slice(&(v as u32).to_be_bytes());
    }
}

fn nc_patch_nonneg(out: &mut [u8], at: usize, off: usize, v: u64) {
    if off == 8 {
        out[at..at + 8].copy_from_slice(&v.to_be_bytes());
    } else {
        out[at..at + 4].copy_from_slice(&(v as u32).to_be_bytes());
    }
}

fn nc_str(out: &mut Vec<u8>, s: &str) {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    nc_push_u32(out, b.len() as u32);
    out.extend_from_slice(&b);
}

fn align4(n: u64) -> u64 {
    n.wrapping_add(3) & !3
}

/// Build a NetCDF classic file: three dimensions (`time` unlimited, `lat` 3, `lon` 4),
/// two global attributes, three variables (two fixed, one record), and two records of
/// record data. Deterministic and byte-exact.
pub fn build_netcdf(version: u8) -> Vec<u8> {
    let off = if version == 2 { 8usize } else { 4usize };
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(if version == 2 { b"CDF\x02" } else { b"CDF\x01" });

    nc_push_nonneg(&mut out, off, 2); // numrecs = 2

    // Dimension list.
    nc_push_u32(&mut out, 10); // NC_DIMENSION
    nc_push_u32(&mut out, 3);
    nc_str(&mut out, "time");
    nc_push_nonneg(&mut out, off, 0); // unlimited -> length 0
    nc_str(&mut out, "lat");
    nc_push_nonneg(&mut out, off, 3);
    nc_str(&mut out, "lon");
    nc_push_nonneg(&mut out, off, 4);

    // Global attribute list.
    nc_push_u32(&mut out, 12); // NC_ATTRIBUTE
    nc_push_u32(&mut out, 2);
    nc_str(&mut out, "title");
    nc_push_u32(&mut out, 2); // NC_CHAR
    nc_push_nonneg(&mut out, off, 4);
    out.extend_from_slice(b"demo");
    nc_str(&mut out, "version");
    nc_push_u32(&mut out, 4); // NC_INT
    nc_push_nonneg(&mut out, off, 1);
    out.extend_from_slice(&1i32.to_be_bytes());

    // Variable list.
    nc_push_u32(&mut out, 11); // NC_VARIABLE
    nc_push_u32(&mut out, 3);

    let mut begins: Vec<usize> = Vec::new();

    // var 0: lat(lat) float -> vsize 12
    nc_str(&mut out, "lat");
    nc_push_u32(&mut out, 1);
    nc_push_u32(&mut out, 1);
    nc_push_u32(&mut out, 0); // vatt absent
    nc_push_u32(&mut out, 5); // NC_FLOAT
    nc_push_nonneg(&mut out, off, 12);
    begins.push(out.len());
    nc_push_nonneg(&mut out, off, 0);

    // var 1: lon(lon) float -> vsize 16
    nc_str(&mut out, "lon");
    nc_push_u32(&mut out, 1);
    nc_push_u32(&mut out, 2);
    nc_push_u32(&mut out, 0);
    nc_push_u32(&mut out, 5);
    nc_push_nonneg(&mut out, off, 16);
    begins.push(out.len());
    nc_push_nonneg(&mut out, off, 0);

    // var 2: temp(time, lat, lon) float, with a `units` attribute -> slab 48
    nc_str(&mut out, "temp");
    nc_push_u32(&mut out, 3);
    nc_push_u32(&mut out, 0);
    nc_push_u32(&mut out, 1);
    nc_push_u32(&mut out, 2);
    nc_push_u32(&mut out, 12); // vatt list
    nc_push_u32(&mut out, 1);
    nc_str(&mut out, "units");
    nc_push_u32(&mut out, 2); // NC_CHAR
    nc_push_nonneg(&mut out, off, 1);
    out.extend_from_slice(b"K\0\0\0");
    nc_push_u32(&mut out, 5); // NC_FLOAT
    nc_push_nonneg(&mut out, off, 48);
    begins.push(out.len());
    nc_push_nonneg(&mut out, off, 0);

    // Lay out the data region: fixed vars first, then the record section.
    let header_len = out.len() as u64;
    let data_start = align4(header_len);
    let record_start = align4(data_start + 12 + 16);
    nc_patch_nonneg(&mut out, begins[0], off, data_start);
    nc_patch_nonneg(&mut out, begins[1], off, data_start + 12);
    nc_patch_nonneg(&mut out, begins[2], off, record_start);

    while (out.len() as u64) < data_start {
        out.push(0);
    }
    out.extend(std::iter::repeat_n(0u8, 12)); // lat
    out.extend(std::iter::repeat_n(0u8, 16)); // lon
    while (out.len() as u64) < record_start {
        out.push(0);
    }
    out.extend(std::iter::repeat_n(0u8, 96)); // 2 records * 48
    out
}

/// A minimal CDF-1 whose single variable declares an out-of-range data `begin`, so the
/// model declines structurally and never reads out of bounds.
pub fn build_netcdf_bad_offset() -> Vec<u8> {
    let off = 4usize;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"CDF\x01");
    nc_push_nonneg(&mut out, off, 0); // numrecs
    nc_push_u32(&mut out, 10); // NC_DIMENSION
    nc_push_u32(&mut out, 1);
    nc_str(&mut out, "x");
    nc_push_nonneg(&mut out, off, 4);
    nc_push_u32(&mut out, 0); // gatt absent
    nc_push_u32(&mut out, 11); // NC_VARIABLE
    nc_push_u32(&mut out, 1);
    nc_str(&mut out, "v");
    nc_push_u32(&mut out, 1); // ndims
    nc_push_u32(&mut out, 0); // dimid 0
    nc_push_u32(&mut out, 0); // vatt absent
    nc_push_u32(&mut out, 5); // NC_FLOAT
    nc_push_nonneg(&mut out, off, 16); // vsize
    nc_push_nonneg(&mut out, off, 0xffff_fff0); // begin: far past EOF
    out
}

// ---------------------------------------------------------------------------
// HDF5 subset writer
// ---------------------------------------------------------------------------

/// An address reference resolved after all pieces are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Ref {
    Eof,
    RootOh,
    RootHeap,
    RootBtree,
    RootSymtab,
    DsetOh,
    DsetData,
    GrpOh,
    GrpHeap,
    GrpBtree,
    GrpSymtab,
    InnerOh,
    InnerData,
}

/// A field to patch with an 8-byte little-endian address.
#[derive(Debug, Clone, Copy)]
enum Patch {
    /// Another piece's start address.
    Addr(Ref),
    /// This piece's own start address plus a delta (a heap's data segment).
    SelfPlus(u64),
}

struct Piece {
    bytes: Vec<u8>,
    /// `(local_offset, patch)`.
    patches: Vec<(usize, Patch)>,
}

struct MsgSpec {
    ty: u16,
    body: Vec<u8>,
    /// `(offset_within_body, patch)`.
    patches: Vec<(usize, Patch)>,
}

fn build_oh(msgs: Vec<MsgSpec>) -> Piece {
    let mut out: Vec<u8> = vec![1, 0]; // version 1, reserved
    let mut patches: Vec<(usize, Patch)> = Vec::new();
    out.extend_from_slice(&le16(msgs.len() as u16));
    out.extend_from_slice(&le32(1)); // ref count
    let msg_bytes: usize = msgs.iter().map(|m| 8 + m.body.len()).sum();
    out.extend_from_slice(&le32(msg_bytes as u32));
    for m in &msgs {
        let body_local = out.len() + 8;
        for (o, p) in &m.patches {
            patches.push((body_local + o, *p));
        }
        out.extend_from_slice(&le16(m.ty));
        out.extend_from_slice(&le16(m.body.len() as u16));
        out.push(0);
        out.extend_from_slice(&[0, 0, 0]);
        out.extend_from_slice(&m.body);
    }
    Piece {
        bytes: out,
        patches,
    }
}

fn symtab_msg(btree: Ref, heap: Ref) -> MsgSpec {
    let mut body = Vec::new();
    body.extend_from_slice(&le64(0));
    body.extend_from_slice(&le64(0));
    MsgSpec {
        ty: 0x0011,
        body,
        patches: vec![(0, Patch::Addr(btree)), (8, Patch::Addr(heap))],
    }
}

fn dataspace_msg(dims: &[u64]) -> MsgSpec {
    let mut body = vec![1u8, dims.len() as u8, 0, 0]; // version, rank, flags, reserved
    for d in dims {
        body.extend_from_slice(&le64(*d));
    }
    MsgSpec {
        ty: 0x0001,
        body,
        patches: vec![],
    }
}

fn datatype_fixed(signed: bool, size: u32) -> MsgSpec {
    let mut body = vec![0x01u8, u8::from(signed), 0, 0]; // version/class, bit field byte 0
    body.extend_from_slice(&le32(size));
    MsgSpec {
        ty: 0x0003,
        body,
        patches: vec![],
    }
}

fn layout_contiguous(data: Ref, size: u64) -> MsgSpec {
    let mut body = vec![1u8, 1, 1, 0, 0, 0, 0, 0]; // version, dimensionality, class=contiguous, reserved
    body.extend_from_slice(&le64(0)); // address (patched)
    body.extend_from_slice(&le64(size)); // size
    MsgSpec {
        ty: 0x0008,
        body,
        patches: vec![(8, Patch::Addr(data))],
    }
}

fn filter_pipeline_msg() -> MsgSpec {
    let mut body = vec![1u8, 1, 0, 0]; // version, number of filters, reserved
    body.extend_from_slice(&le16(1)); // filter id = deflate
    body.extend_from_slice(&le16(0)); // name length
    body.extend_from_slice(&le16(0)); // flags
    body.extend_from_slice(&le16(0)); // number of client values
    MsgSpec {
        ty: 0x000B,
        body,
        patches: vec![],
    }
}

fn attribute_msg(name: &str, value: i32) -> MsgSpec {
    let dt = datatype_fixed(true, 4);
    let ds = dataspace_msg(&[]);
    let mut name_bytes = name.as_bytes().to_vec();
    name_bytes.push(0);
    let mut body = vec![1u8, 0]; // version, reserved
    body.extend_from_slice(&le16(name_bytes.len() as u16));
    body.extend_from_slice(&le16(dt.body.len() as u16));
    body.extend_from_slice(&le16(ds.body.len() as u16));
    body.extend_from_slice(&name_bytes);
    body.extend_from_slice(&dt.body);
    body.extend_from_slice(&ds.body);
    body.extend_from_slice(&value.to_le_bytes());
    MsgSpec {
        ty: 0x000C,
        body,
        patches: vec![],
    }
}

/// The heap data segment (offset 0 is a NUL; each name is NUL-terminated).
/// Returns `(data, name_offsets)`.
fn heap_data(names: &[&str]) -> (Vec<u8>, Vec<u64>) {
    let mut data = vec![0u8];
    let mut offs = Vec::new();
    for n in names {
        offs.push(data.len() as u64);
        data.extend_from_slice(n.as_bytes());
        data.push(0);
    }
    (data, offs)
}

/// Build a local heap (header + data segment). The data-segment address field is
/// patched to this piece's own start + the header length.
fn heap_piece(names: &[&str]) -> (Piece, Vec<u64>) {
    let (data, offs) = heap_data(names);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"HEAP");
    bytes.extend_from_slice(&[0, 0, 0, 0]); // version + reserved
    bytes.extend_from_slice(&le64(data.len() as u64)); // data segment size
    bytes.extend_from_slice(&le64(0)); // free-list head
    let addr_pos = bytes.len();
    bytes.extend_from_slice(&le64(0)); // data segment address (patched)
    let header_len = bytes.len() as u64;
    bytes.extend_from_slice(&data);
    (
        Piece {
            bytes,
            patches: vec![(addr_pos, Patch::SelfPlus(header_len))],
        },
        offs,
    )
}

/// Build a type-0 B-tree with `children` immediate children and no keys.
fn btree_piece(children: &[Ref]) -> Piece {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0, 0]); // node type 0, level 0
    bytes.extend_from_slice(&le16(children.len().saturating_sub(1) as u16)); // keys
    bytes.extend_from_slice(&le64(UNDEF)); // left
    bytes.extend_from_slice(&le64(UNDEF)); // right
    let mut patches = Vec::new();
    // Layout: key_i at base + 2i*off, child_i at base + (2i+1)*off for i in 0..n,
    // then child_n at base + 2n*off. With n = children.len()-1 keys, the children
    // occupy the interleaved slots.
    for (i, c) in children.iter().enumerate() {
        let at = bytes.len();
        bytes.extend_from_slice(&le64(0));
        patches.push((at, Patch::Addr(*c)));
        if i + 1 < children.len() {
            // A placeholder key precedes each subsequent child pointer.
            bytes.extend_from_slice(&le64(0));
        }
    }
    Piece { bytes, patches }
}

fn symtab_piece(entries: &[(u64, Ref)]) -> Piece {
    let k = 4usize; // group leaf K from the superblock
    let mut bytes = vec![0u8; 2 * k * 40];
    let mut patches = Vec::new();
    for (slot, (link_off, oh)) in entries.iter().enumerate() {
        let at = slot * 40;
        bytes[at..at + 8].copy_from_slice(&le64(*link_off));
        patches.push((at + 8, Patch::Addr(*oh)));
        bytes[at + 16..at + 20].copy_from_slice(&le32(1)); // cache type = group
    }
    Piece { bytes, patches }
}

/// Build the HDF5 file with the given user-block base (0 or 512) and controls.
pub fn build_hdf5(base: u64, filtered: bool, dset_oh_v2: bool) -> Vec<u8> {
    // Superblock (96 bytes): signature + v0 fields + root STE.
    let mut sb = vec![0u8; 96];
    sb[0..8].copy_from_slice(&HDF5_SIG);
    sb[8] = 0; // superblock version 0
    sb[13] = 8; // size of offsets
    sb[14] = 8; // size of lengths
    sb[16..18].copy_from_slice(&le16(4)); // group leaf K
    sb[18..20].copy_from_slice(&le16(16)); // group internal K
    let sb_piece = Piece {
        bytes: sb,
        patches: vec![(40, Patch::Addr(Ref::Eof)), (64, Patch::Addr(Ref::RootOh))],
    };

    let (root_heap, root_offs) = heap_piece(&["dset", "grp"]);
    let (grp_heap, grp_offs) = heap_piece(&["inner"]);

    let root_oh = build_oh(vec![
        symtab_msg(Ref::RootBtree, Ref::RootHeap),
        attribute_msg("root_attr", 42),
    ]);
    let grp_oh = build_oh(vec![symtab_msg(Ref::GrpBtree, Ref::GrpHeap)]);

    // The root group links `dset` (dataset) and `grp` (group).
    let root_symtab = symtab_piece(&[(root_offs[0], Ref::DsetOh), (root_offs[1], Ref::GrpOh)]);
    let root_btree = btree_piece(&[Ref::RootSymtab]);
    // The subgroup links `inner` (dataset).
    let grp_symtab = symtab_piece(&[(grp_offs[0], Ref::InnerOh)]);
    let grp_btree = btree_piece(&[Ref::GrpSymtab]);

    let mut dset_msgs = vec![
        dataspace_msg(&[4]),
        datatype_fixed(true, 4),
        layout_contiguous(Ref::DsetData, 16),
        attribute_msg("order", 7),
    ];
    if filtered {
        dset_msgs.push(filter_pipeline_msg());
    }
    let mut dset_oh = build_oh(dset_msgs);
    if dset_oh_v2 {
        dset_oh.bytes[0] = 2;
    }

    let inner_oh = build_oh(vec![
        dataspace_msg(&[2]),
        datatype_fixed(true, 4),
        layout_contiguous(Ref::InnerData, 8),
    ]);

    let dset_data = Piece {
        bytes: vec![0u8; 16],
        patches: vec![],
    };
    let inner_data = Piece {
        bytes: vec![0u8; 8],
        patches: vec![],
    };

    let order: Vec<(Ref, Piece)> = vec![
        (Ref::RootOh, root_oh),
        (Ref::RootHeap, root_heap),
        (Ref::RootBtree, root_btree),
        (Ref::RootSymtab, root_symtab),
        (Ref::DsetOh, dset_oh),
        (Ref::DsetData, dset_data),
        (Ref::GrpOh, grp_oh),
        (Ref::GrpHeap, grp_heap),
        (Ref::GrpBtree, grp_btree),
        (Ref::GrpSymtab, grp_symtab),
        (Ref::InnerOh, inner_oh),
        (Ref::InnerData, inner_data),
    ];

    let mut buf = vec![0u8; base as usize];
    let sb_start = buf.len();
    buf.extend_from_slice(&sb_piece.bytes);
    let mut placed: Vec<(usize, Vec<(usize, Patch)>)> = vec![(sb_start, sb_piece.patches.clone())];
    let mut addr: HashMap<Ref, u64> = HashMap::new();
    for (r, p) in order {
        let start = buf.len();
        addr.insert(r, start as u64);
        placed.push((start, p.patches.clone()));
        buf.extend_from_slice(&p.bytes);
    }
    addr.insert(Ref::Eof, buf.len() as u64);
    for (start, patches) in &placed {
        for (off, patch) in patches {
            let value = match patch {
                Patch::Addr(r) => *addr.get(r).expect("address ref resolved"),
                Patch::SelfPlus(d) => *start as u64 + d,
            };
            buf[start + off..start + off + 8].copy_from_slice(&le64(value));
        }
    }
    buf
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(dir.join(name), bytes)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let out_dir = args
        .next()
        .unwrap_or_else(|| "evidence/scratch/phase21-34/corpus".to_string());
    let dir = Path::new(&out_dir);
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("cannot create {out_dir}: {e}");
        return ExitCode::FAILURE;
    }

    let cdf1 = build_netcdf(1);
    let cdf2 = build_netcdf(2);
    let cdf_badoffset = build_netcdf_bad_offset();
    // A CDF magic with a truncated header -> detected as NetCDF, model declines typed.
    let mut nc_truncated = b"CDF\x01".to_vec();
    nc_truncated.extend_from_slice(&[0x00, 0x00]);
    // A CDF magic with a bad dim-list tag -> detected, model declines typed.
    let mut nc_junk = b"CDF\x01".to_vec();
    nc_junk.extend_from_slice(&0u32.to_be_bytes());
    nc_junk.extend_from_slice(&0xdead_beefu32.to_be_bytes());

    let h5_basic = build_hdf5(0, false, false);
    let h5_userblock = build_hdf5(512, false, false);
    let h5_filtered = build_hdf5(0, true, false);
    let h5_ohv2 = build_hdf5(0, false, true);

    let mut h5_badmagic = HDF5_SIG.to_vec();
    h5_badmagic.push(7); // invalid superblock version
    h5_badmagic.resize(160, 0);
    let h5_truncated = HDF5_SIG.to_vec(); // signature only
    let mut h5_junk = HDF5_SIG.to_vec();
    h5_junk.resize(160, 0); // version 0 but zero offset/length sizes -> not detected
    let prose =
        b"The quick brown fox jumps over the lazy dog.\nPlain prose, not a scientific container.\n"
            .to_vec();

    let fixtures: &[(&str, &[u8])] = &[
        ("netcdf_cdf1.nc", &cdf1),
        ("netcdf_cdf2.nc", &cdf2),
        ("netcdf_badoffset.nc", &cdf_badoffset),
        ("nc_truncated.nc", &nc_truncated),
        ("nc_junk.nc", &nc_junk),
        ("hdf5_basic.h5", &h5_basic),
        ("hdf5_userblock.h5", &h5_userblock),
        ("hdf5_filtered.h5", &h5_filtered),
        ("hdf5_ohv2.h5", &h5_ohv2),
        ("hdf5_badmagic.bin", &h5_badmagic),
        ("hdf5_truncated.h5", &h5_truncated),
        ("hdf5_junk.bin", &h5_junk),
        ("prose.txt", &prose),
    ];
    for (name, bytes) in fixtures {
        if let Err(e) = write(dir, name, bytes) {
            eprintln!("cannot write {name}: {e}");
            return ExitCode::FAILURE;
        }
    }
    println!("wrote {} fixtures to {}", fixtures.len(), out_dir);
    ExitCode::SUCCESS
}
