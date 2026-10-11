//! Phase 26.4 legacy Outlook message (`.msg`, MS-OXMSG) court fixture generator.
//!
//! Dependency-free: it writes small, deterministic `.msg` compound files (a root
//! `__properties_version1.0` with a message header and a fixed-size property set, a
//! `__substg1.0_*` Unicode/ANSI/binary substream set, a `__nameid_version1.0` stream,
//! one recipient and one attachment sub-storage each with their own property list and
//! substreams, plus malformed and non-message controls) byte-for-byte, so the court
//! needs no Outlook, LibreOffice, or OLE tooling and no network.
//!
//! This file mirrors `examples/ppt_fixtures.rs`: the CFBF v3 writer is extended to a
//! directory **tree** (storages with children), and only the `.msg` payloads differ.
//!
//! ```sh
//! cargo run --locked --all-features --example msg_fixtures -- CORPUS_DIR
//! ```

use std::path::Path;

// ---------------------------------------------------------------------------
// CFBF v3 writer (all streams live in the root mini stream)
// ---------------------------------------------------------------------------

const SIG: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_SS: usize = 64;
const SECTOR: usize = 512;
const MINI_CUTOFF: u32 = 4096;

const OBJ_STORAGE: u8 = 1;
const OBJ_STREAM: u8 = 2;
const OBJ_ROOT: u8 = 5;

/// A directory entry to write (a stream or a storage with children).
enum Ent {
    Stream(String, Vec<u8>),
    Storage(String, Vec<Ent>),
}

struct DirNode {
    name: String,
    obj_type: u8,
    start: u32,
    size: u64,
    child: u32,
    right: u32,
}

fn utf16le(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for u in name.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn pad64(d: &[u8]) -> Vec<u8> {
    let mut v = d.to_vec();
    let r = v.len() % MINI_SS;
    if r != 0 {
        v.resize(v.len() + (MINI_SS - r), 0);
    }
    v
}

fn dir_entry_bytes(n: &DirNode) -> [u8; 128] {
    let mut e = [0u8; 128];
    let name = utf16le(&n.name);
    assert!(name.len() <= 64, "CFBF name is too long: {}", n.name);
    e[..name.len()].copy_from_slice(&name);
    e[64..66].copy_from_slice(&(name.len() as u16).to_le_bytes());
    e[66] = n.obj_type;
    e[67] = 1; // black (a right-chain has no balancing requirement for our layout)
    e[68..72].copy_from_slice(&NOSTREAM.to_le_bytes()); // left
    e[72..76].copy_from_slice(&n.right.to_le_bytes());
    e[76..80].copy_from_slice(&n.child.to_le_bytes());
    e[116..120].copy_from_slice(&n.start.to_le_bytes());
    e[120..128].copy_from_slice(&n.size.to_le_bytes());
    e
}

/// Assign directory indices (pre-order sibling blocks) to `entries`, laying every
/// stream's content into the mini stream. Returns the first sibling's index.
fn build_nodes(entries: &[Ent], out: &mut Vec<DirNode>, mini: &mut Vec<u8>) -> u32 {
    if entries.is_empty() {
        return NOSTREAM;
    }
    let start = out.len() as u32;
    for _ in entries {
        out.push(DirNode {
            name: String::new(),
            obj_type: 0,
            start: 0,
            size: 0,
            child: NOSTREAM,
            right: NOSTREAM,
        });
    }
    for (k, e) in entries.iter().enumerate() {
        let idx = (start as usize) + k;
        let right = if k + 1 < entries.len() {
            start + k as u32 + 1
        } else {
            NOSTREAM
        };
        out[idx] = match e {
            Ent::Stream(name, data) => {
                let s = (mini.len() / MINI_SS) as u32;
                mini.extend_from_slice(&pad64(data));
                DirNode {
                    name: name.clone(),
                    obj_type: OBJ_STREAM,
                    start: s,
                    size: data.len() as u64,
                    child: NOSTREAM,
                    right,
                }
            }
            Ent::Storage(name, children) => {
                let child = build_nodes(children, out, mini);
                DirNode {
                    name: name.clone(),
                    obj_type: OBJ_STORAGE,
                    start: 0,
                    size: 0,
                    child,
                    right,
                }
            }
        };
    }
    start
}

/// Build a CFBF v3 compound file whose streams all live in the root mini stream.
fn build_cfbf(root_children: &[Ent]) -> Vec<u8> {
    let epf = SECTOR / 4;
    let epd = SECTOR / 128;

    let mut nodes: Vec<DirNode> = Vec::new();
    nodes.push(DirNode {
        name: String::new(),
        obj_type: OBJ_ROOT,
        start: 0,
        size: 0,
        child: NOSTREAM,
        right: NOSTREAM,
    });
    let mut mini: Vec<u8> = Vec::new();
    let child = build_nodes(root_children, &mut nodes, &mut mini);
    nodes[0] = DirNode {
        name: "Root Entry".to_string(),
        obj_type: OBJ_ROOT,
        start: 0,
        size: mini.len() as u64,
        child,
        right: NOSTREAM,
    };

    let mini_secs = mini.len() / MINI_SS;
    let minifat_secs = mini_secs.div_ceil(epf);
    let mut minifat = vec![FREESECT; minifat_secs * epf];
    for n in &nodes {
        if n.obj_type != OBJ_STREAM || n.size == 0 {
            continue;
        }
        let count = (n.size as usize).div_ceil(MINI_SS);
        for j in 0..count {
            minifat[n.start as usize + j] = if j + 1 < count {
                n.start + j as u32 + 1
            } else {
                ENDOFCHAIN
            };
        }
    }

    let mini_stream_secs = mini.len().div_ceil(SECTOR).max(1);
    let dir_secs = nodes.len().div_ceil(epd);
    let fat_secs = 1usize;

    let mini_stream_start = 0usize;
    let minifat_start = mini_stream_secs;
    let dir_start = minifat_start + minifat_secs;
    let fat_start = dir_start + dir_secs;
    let total = fat_start + fat_secs;
    assert!(total <= epf, "fixture is too large for one FAT sector");

    let mut fat = vec![FREESECT; epf];
    for i in 0..mini_stream_secs {
        fat[mini_stream_start + i] = if i + 1 < mini_stream_secs {
            (mini_stream_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    for i in 0..minifat_secs {
        fat[minifat_start + i] = if i + 1 < minifat_secs {
            (minifat_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    for i in 0..dir_secs {
        fat[dir_start + i] = if i + 1 < dir_secs {
            (dir_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    for i in 0..fat_secs {
        fat[fat_start + i] = FATSECT;
    }

    let mut out = vec![0u8; SECTOR];
    out[..8].copy_from_slice(&SIG);
    out[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    out[26..28].copy_from_slice(&3u16.to_le_bytes());
    out[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    out[30..32].copy_from_slice(&9u16.to_le_bytes());
    out[32..34].copy_from_slice(&6u16.to_le_bytes());
    out[40..44].copy_from_slice(&0u32.to_le_bytes());
    out[44..48].copy_from_slice(&(fat_secs as u32).to_le_bytes());
    out[48..52].copy_from_slice(&(dir_start as u32).to_le_bytes());
    out[56..60].copy_from_slice(&MINI_CUTOFF.to_le_bytes());
    out[60..64].copy_from_slice(&(minifat_start as u32).to_le_bytes());
    out[64..68].copy_from_slice(&(minifat_secs as u32).to_le_bytes());
    out[68..72].copy_from_slice(&ENDOFCHAIN.to_le_bytes());
    out[72..76].copy_from_slice(&0u32.to_le_bytes());
    out[76..80].copy_from_slice(&(fat_start as u32).to_le_bytes());
    for i in 1..109 {
        out[76 + i * 4..80 + i * 4].copy_from_slice(&FREESECT.to_le_bytes());
    }

    let mut sectors: Vec<Vec<u8>> = Vec::new();
    for i in 0..mini_stream_secs {
        let mut s = vec![0u8; SECTOR];
        let off = i * SECTOR;
        let end = (off + SECTOR).min(mini.len());
        if end > off {
            s[..end - off].copy_from_slice(&mini[off..end]);
        }
        sectors.push(s);
    }
    for sec in 0..minifat_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epf {
            let idx = sec * epf + k;
            if idx >= minifat.len() {
                break;
            }
            s[k * 4..k * 4 + 4].copy_from_slice(&minifat[idx].to_le_bytes());
        }
        sectors.push(s);
    }
    for sec in 0..dir_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epd {
            let idx = sec * epd + k;
            if idx >= nodes.len() {
                break;
            }
            let off = k * 128;
            s[off..off + 128].copy_from_slice(&dir_entry_bytes(&nodes[idx]));
        }
        sectors.push(s);
    }
    for sec in 0..fat_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epf {
            let v = fat[sec * epf + k];
            s[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sectors.push(s);
    }
    for s in &sectors {
        out.extend_from_slice(s);
    }
    out
}

// ---------------------------------------------------------------------------
// MS-OXMSG writer
// ---------------------------------------------------------------------------

const STREAM_PROPERTIES: &str = "__properties_version1.0";
const STREAM_NAMEID: &str = "__nameid_version1.0";

const PT_UNICODE: u16 = 0x001F;
const PT_STRING8: u16 = 0x001E;
const PT_BINARY: u16 = 0x0102;
const PT_SYSTIME: u16 = 0x0040;

const FLAG_VARIABLE: u32 = 0x0000_0004;
const FLAG_FIXED: u32 = 0x0000_0000;

/// A 16-byte fixed-size property entry.
fn entry(tag: u32, flags: u32, value: [u8; 8]) -> [u8; 16] {
    let mut e = [0u8; 16];
    e[0..4].copy_from_slice(&tag.to_le_bytes());
    e[4..8].copy_from_slice(&flags.to_le_bytes());
    e[8..16].copy_from_slice(&value);
    e
}

/// A variable-size property entry whose 8-byte value field holds the substream size.
fn var_entry(prop_type: u16, id: u16, size: u32) -> [u8; 16] {
    let tag = ((id as u32) << 16) | prop_type as u32;
    let mut value = [0u8; 8];
    value[0..4].copy_from_slice(&size.to_le_bytes());
    entry(tag, FLAG_VARIABLE, value)
}

/// A fixed-size property entry with a raw 8-byte value.
fn fixed_entry(prop_type: u16, id: u16, value: [u8; 8]) -> [u8; 16] {
    let tag = ((id as u32) << 16) | prop_type as u32;
    entry(tag, FLAG_FIXED, value)
}

fn substream_name(prop_type: u16, id: u16) -> String {
    format!("__substg1.0_{id:04X}{prop_type:04X}")
}

fn utf16_units(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

const SUBJECT: &str = "Hello Ω";
const BODY: &str = "Body text\nSecond line";
const SENDER_ANSI: &[u8] = b"Caf\xE9 \x80"; // CP1252 for "Café €"
const MESSAGE_CLASS: &str = "IPM.Note";
const BINARY_VALUE: &[u8] = b"\x00\x01\x02\xFF\xFE";
const RECIP_DISPLAY: &str = "Carol <carol@example.test>";
const ATTACH_SHORT: &[u8] = b"file.txt";
const ATTACH_LONG: &str = "long file.txt";
const ATTACH_MIME: &[u8] = b"text/plain";
const ATTACH_DATA: &[u8] = b"\xCA\xFE\xBA\xBE";

/// The fixture options (all default to a *valid* message).
#[derive(Clone, Copy, Default)]
struct Opts {
    /// The message `__properties_version1.0` is truncated below its 32-byte header.
    short_props: bool,
    /// Append 4 stray bytes, so a fixed property entry overruns the stream.
    entry_overrun: bool,
    /// A root substream name is not `PPPPTTTT` hex.
    bad_substream_name: bool,
    /// Add a root substream that no fixed entry references.
    unreferenced_substream: bool,
    /// Add a referenced root substream with an unsupported property type (PT_SYSTIME).
    unsupported_substream: bool,
    /// The header declares 2 recipients but only one storage exists.
    count_mismatch: bool,
    /// The recipient storage omits its own `__properties_version1.0`.
    recip_no_props: bool,
    /// The recipient storage name suffix is not decimal.
    bad_recip_suffix: bool,
    /// Omit the root `__properties_version1.0` (so detection is `cfbf`).
    missing_props: bool,
}

fn message_props(o: &Opts) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    // Header: Reserved(8), NextRecipientId, NextAttachmentId, RecipientCount,
    // AttachmentCount, Reserved(8).
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    let recip_count: u32 = if o.count_mismatch { 2 } else { 1 };
    out.extend_from_slice(&recip_count.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    // Fixed-size property entries.
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x001A,
        MESSAGE_CLASS.len() as u32 * 2,
    ));
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x0037,
        SUBJECT.encode_utf16().count() as u32 * 2,
    ));
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x1000,
        BODY.encode_utf16().count() as u32 * 2,
    ));
    out.extend_from_slice(&var_entry(PT_STRING8, 0x0C1A, SENDER_ANSI.len() as u32));
    out.extend_from_slice(&var_entry(PT_BINARY, 0x1234, BINARY_VALUE.len() as u32));
    out.extend_from_slice(&fixed_entry(PT_SYSTIME, 0x0039, [0x11; 8]));
    if o.short_props {
        out.truncate(24);
    }
    if o.entry_overrun {
        out.extend_from_slice(&[0u8; 4]);
    }
    out
}

fn recipient_props() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&[0u8; 8]); // Reserved(8)
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x0E04,
        RECIP_DISPLAY.encode_utf16().count() as u32 * 2,
    ));
    out.extend_from_slice(&fixed_entry(
        0x0003,
        0x0C15,
        [0x01, 0x00, 0x00, 0x00, 0, 0, 0, 0],
    ));
    out
}

fn attachment_props() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&[0u8; 8]); // Reserved(8)
    out.extend_from_slice(&var_entry(PT_STRING8, 0x3704, ATTACH_SHORT.len() as u32));
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x3707,
        ATTACH_LONG.encode_utf16().count() as u32 * 2,
    ));
    out.extend_from_slice(&var_entry(PT_STRING8, 0x370E, ATTACH_MIME.len() as u32));
    out.extend_from_slice(&var_entry(PT_BINARY, 0x3701, ATTACH_DATA.len() as u32));
    out
}

fn root_children(o: &Opts) -> Vec<Ent> {
    let mut v: Vec<Ent> = Vec::new();
    if !o.missing_props {
        v.push(Ent::Stream(STREAM_PROPERTIES.to_string(), message_props(o)));
    }
    v.push(Ent::Stream(
        STREAM_NAMEID.to_string(),
        b"\x01\x00\x00\x00nameid".to_vec(),
    ));
    // Variable-size substreams (unicode, ANSI, binary).
    v.push(Ent::Stream(
        substream_name(PT_UNICODE, 0x001A),
        utf16_units(MESSAGE_CLASS),
    ));
    v.push(Ent::Stream(
        substream_name(PT_UNICODE, 0x0037),
        utf16_units(SUBJECT),
    ));
    v.push(Ent::Stream(
        substream_name(PT_UNICODE, 0x1000),
        utf16_units(BODY),
    ));
    v.push(Ent::Stream(
        substream_name(PT_STRING8, 0x0C1A),
        SENDER_ANSI.to_vec(),
    ));
    v.push(Ent::Stream(
        substream_name(PT_BINARY, 0x1234),
        BINARY_VALUE.to_vec(),
    ));
    if o.bad_substream_name {
        v.push(Ent::Stream(
            "__substg1.0_ZZZZ001F".to_string(),
            utf16_units("bad name"),
        ));
    }
    if o.unreferenced_substream {
        v.push(Ent::Stream(
            substream_name(PT_UNICODE, 0x9999),
            utf16_units("unreferenced"),
        ));
    }
    if o.unsupported_substream {
        // 0x00390040 is a referenced fixed entry (PT_SYSTIME) but its type is not
        // decodable as a substream.
        v.push(Ent::Stream(
            substream_name(PT_SYSTIME, 0x0039),
            vec![0u8; 8],
        ));
    }
    // One recipient sub-storage.
    let recip_name = if o.bad_recip_suffix {
        "__recip_version1.0_x".to_string()
    } else {
        "__recip_version1.0_1".to_string()
    };
    let mut recip_children: Vec<Ent> = Vec::new();
    if !o.recip_no_props {
        recip_children.push(Ent::Stream(
            STREAM_PROPERTIES.to_string(),
            recipient_props(),
        ));
    }
    recip_children.push(Ent::Stream(
        substream_name(PT_UNICODE, 0x0E04),
        utf16_units(RECIP_DISPLAY),
    ));
    v.push(Ent::Storage(recip_name, recip_children));
    // One attachment sub-storage.
    let attach_children: Vec<Ent> = vec![
        Ent::Stream(STREAM_PROPERTIES.to_string(), attachment_props()),
        Ent::Stream(substream_name(PT_STRING8, 0x3704), ATTACH_SHORT.to_vec()),
        Ent::Stream(substream_name(PT_UNICODE, 0x3707), utf16_units(ATTACH_LONG)),
        Ent::Stream(substream_name(PT_STRING8, 0x370E), ATTACH_MIME.to_vec()),
        Ent::Stream(substream_name(PT_BINARY, 0x3701), ATTACH_DATA.to_vec()),
    ];
    v.push(Ent::Storage(
        "__attach_version1.0_1".to_string(),
        attach_children,
    ));
    v
}

fn build_msg(o: &Opts) -> Vec<u8> {
    build_cfbf(&root_children(o))
}

/// A generic compound file with no root `__properties_version1.0`.
fn build_generic_cfbf() -> Vec<u8> {
    build_cfbf(&[
        Ent::Stream("Data".to_string(), b"not a message".to_vec()),
        Ent::Stream("Meta".to_string(), b"generic compound file".to_vec()),
    ])
}

fn write(dir: &str, name: &str, bytes: &[u8]) {
    let path = Path::new(dir).join(name);
    std::fs::write(&path, bytes).expect("write fixture");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("usage: msg_fixtures OUTDIR");
    std::fs::create_dir_all(dir).expect("create outdir");

    let valid = build_msg(&Opts::default());
    write(dir, "msg_valid.msg", &valid);

    for (o, name) in [
        (
            Opts {
                short_props: true,
                ..Opts::default()
            },
            "msg_short_props.msg",
        ),
        (
            Opts {
                entry_overrun: true,
                ..Opts::default()
            },
            "msg_entry_overrun.msg",
        ),
        (
            Opts {
                bad_substream_name: true,
                ..Opts::default()
            },
            "msg_bad_substream_name.msg",
        ),
        (
            Opts {
                unreferenced_substream: true,
                ..Opts::default()
            },
            "msg_unreferenced_substream.msg",
        ),
        (
            Opts {
                unsupported_substream: true,
                ..Opts::default()
            },
            "msg_unsupported_substream.msg",
        ),
        (
            Opts {
                count_mismatch: true,
                ..Opts::default()
            },
            "msg_count_mismatch.msg",
        ),
        (
            Opts {
                recip_no_props: true,
                ..Opts::default()
            },
            "msg_recip_no_props.msg",
        ),
        (
            Opts {
                bad_recip_suffix: true,
                ..Opts::default()
            },
            "msg_bad_recip_suffix.msg",
        ),
    ] {
        write(dir, name, &build_msg(&o));
    }

    // A compound file that is not a message at all (no root __properties_version1.0).
    write(
        dir,
        "msg_missing_props.msg",
        &build_msg(&Opts {
            missing_props: true,
            ..Opts::default()
        }),
    );
    write(dir, "generic_cfbf.bin", &build_generic_cfbf());
    write(
        dir,
        "prose.txt",
        b"The quick brown fox jumps over the lazy dog.\nPlain prose, not a message.\n",
    );

    println!(
        "wrote msg fixtures (valid={} bytes) into {dir}",
        valid.len()
    );
}
