//! Bounded WordprocessingML parsing and the canonical [`StoryModel`] (Phase 12.4).
//!
//! This is **derived (`Q_gen`)** state only. It parses one story part into a
//! declared subset of WordprocessingML (ISO/IEC 29500-1 clause 17): paragraphs,
//! runs and text (`w:t`, `w:delText`, `xml:space`), paragraph/run styles with
//! **heading identity resolved through `outlineLvl`/`basedOn`** (never locale
//! names), tables/rows/cells with `gridSpan`/`vMerge`, hyperlinks, bookmarks,
//! section boundaries, fields, tracked changes, and drawing resource references.
//!
//! Anything not modelled is simply not interpreted; the exact member bytes remain
//! authoritative. XML parsing is hardened exactly like the OPC core: UTF-8 only,
//! no DTD/entities, bounded depth/events/nodes/attributes/text, and a hard root
//! element check (`w:document`, `w:hdr`, `w:ftr`, `w:footnotes`, `w:endnotes`,
//! `w:comments`).

use std::collections::BTreeMap;

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::error::{Error, Result};
use crate::limits::Limits;

use super::{
    ByteReader, DocxExtractProfile, DocxStory, FieldMode, StyleTable, put_opt_str, put_str, put_u32,
};

/// One text run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// 0-based index within its paragraph.
    pub index: u32,
    /// Profile-resolved run text.
    pub text: String,
    /// `w:rStyle` value, when present.
    pub style_id: Option<String>,
}

/// One paragraph (body-level; cell paragraphs are held by their cell).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    /// 0-based index among body-level paragraphs in document order.
    pub index: u32,
    /// Profile-resolved text.
    pub text: String,
    /// `w:pStyle` value, when present.
    pub style_id: Option<String>,
    /// Resolved 0-based outline level (heading identity), when the style has one.
    pub heading_level: Option<u8>,
    /// The runs, in order.
    pub runs: Vec<Run>,
}

/// One table cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// 0-based grid column (physical `w:tc` position).
    pub grid_col: u32,
    /// `w:gridSpan` (>= 1).
    pub grid_span: u32,
    /// Whether this cell continues a vertical merge (`w:vMerge` without `restart`).
    pub vmerge_continue: bool,
    /// Profile-resolved cell text (paragraphs joined by `\n`).
    pub text: String,
}

/// One table row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Cells, in order.
    pub cells: Vec<Cell>,
}

/// One table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// 0-based index among top-level tables.
    pub index: u32,
    /// Rows, in order.
    pub rows: Vec<Row>,
}

impl Table {
    /// The table's text: rows joined by `\n`, cells by `\t`.
    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A block-level element in a story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A paragraph.
    Paragraph(Paragraph),
    /// A table.
    Table(Table),
}

/// A hyperlink (`w:hyperlink`), with its text and either a relationship id or an
/// internal anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hyperlink {
    /// Hyperlink text (the runs inside it).
    pub text: String,
    /// `r:id` (external relationship), when present.
    pub rel_id: Option<String>,
    /// `w:anchor` (internal bookmark), when present.
    pub anchor: Option<String>,
}

/// The canonical parsed story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryModel {
    /// The story this model belongs to.
    pub story: DocxStory,
    /// The absolute part name.
    pub part_name: String,
    /// The verified root element local name.
    pub root_local: String,
    /// The story's blocks, in document order.
    pub blocks: Vec<Block>,
    /// Hyperlinks, in order.
    pub hyperlinks: Vec<Hyperlink>,
    /// Bookmark names, in order.
    pub bookmarks: Vec<String>,
    /// Drawing resource relationship ids (`r:embed`/`r:id`), in order.
    pub resources: Vec<String>,
    /// Number of `w:sectPr` boundaries.
    pub section_count: u32,
    /// Field instruction codes seen.
    pub fields: Vec<String>,
}

impl StoryModel {
    /// Body-level paragraphs, in document order.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.blocks.iter().filter_map(|b| match b {
            Block::Paragraph(p) => Some(p),
            Block::Table(_) => None,
        })
    }

    /// Top-level tables, in document order.
    pub fn tables(&self) -> impl Iterator<Item = &Table> {
        self.blocks.iter().filter_map(|b| match b {
            Block::Table(t) => Some(t),
            Block::Paragraph(_) => None,
        })
    }

    /// The story's full text: block texts joined by `\n`.
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .map(|b| match b {
                Block::Paragraph(p) => p.text.clone(),
                Block::Table(t) => t.text(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Canonical encode.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"DSTM");
        out.push(1);
        out.push(self.story.tag());
        put_u32(&mut out, story_index(self.story));
        put_str(&mut out, &self.part_name);
        put_str(&mut out, &self.root_local);
        put_u32(&mut out, self.blocks.len() as u32);
        for b in &self.blocks {
            match b {
                Block::Paragraph(p) => {
                    out.push(0);
                    put_u32(&mut out, p.index);
                    put_opt_str(&mut out, p.style_id.as_deref());
                    out.push(p.heading_level.unwrap_or(0xFF));
                    put_str(&mut out, &p.text);
                    put_u32(&mut out, p.runs.len() as u32);
                    for r in &p.runs {
                        put_u32(&mut out, r.index);
                        put_str(&mut out, &r.text);
                        put_opt_str(&mut out, r.style_id.as_deref());
                    }
                }
                Block::Table(t) => {
                    out.push(1);
                    put_u32(&mut out, t.index);
                    put_u32(&mut out, t.rows.len() as u32);
                    for row in &t.rows {
                        put_u32(&mut out, row.cells.len() as u32);
                        for c in &row.cells {
                            put_u32(&mut out, c.grid_col);
                            put_u32(&mut out, c.grid_span);
                            out.push(c.vmerge_continue as u8);
                            put_str(&mut out, &c.text);
                        }
                    }
                }
            }
        }
        put_u32(&mut out, self.hyperlinks.len() as u32);
        for h in &self.hyperlinks {
            put_str(&mut out, &h.text);
            put_opt_str(&mut out, h.rel_id.as_deref());
            put_opt_str(&mut out, h.anchor.as_deref());
        }
        put_u32(&mut out, self.bookmarks.len() as u32);
        for b in &self.bookmarks {
            put_str(&mut out, b);
        }
        put_u32(&mut out, self.resources.len() as u32);
        for r in &self.resources {
            put_str(&mut out, r);
        }
        put_u32(&mut out, self.section_count);
        put_u32(&mut out, self.fields.len() as u32);
        for f in &self.fields {
            put_str(&mut out, f);
        }
        out
    }

    /// Decode a model produced by [`StoryModel::encode`].
    pub fn decode(bytes: &[u8]) -> Result<StoryModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"DSTM" {
            return Err(corrupt("bad story model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported story model version"));
        }
        let tag = r.u8()?;
        let index = r.u32()?;
        let story = story_from_tag(tag, index)?;
        let part_name = r.string()?;
        let root_local = r.string()?;
        let nb = r.u32()?;
        let mut blocks = Vec::new();
        for _ in 0..nb {
            match r.u8()? {
                0 => {
                    let index = r.u32()?;
                    let style_id = r.opt_string()?;
                    let h = r.u8()?;
                    let heading_level = if h == 0xFF { None } else { Some(h) };
                    let text = r.string()?;
                    let nr = r.u32()?;
                    let mut runs = Vec::new();
                    for _ in 0..nr {
                        let index = r.u32()?;
                        let text = r.string()?;
                        let style_id = r.opt_string()?;
                        runs.push(Run {
                            index,
                            text,
                            style_id,
                        });
                    }
                    blocks.push(Block::Paragraph(Paragraph {
                        index,
                        text,
                        style_id,
                        heading_level,
                        runs,
                    }));
                }
                1 => {
                    let index = r.u32()?;
                    let nrows = r.u32()?;
                    let mut rows = Vec::new();
                    for _ in 0..nrows {
                        let ncells = r.u32()?;
                        let mut cells = Vec::new();
                        for _ in 0..ncells {
                            let grid_col = r.u32()?;
                            let grid_span = r.u32()?;
                            let vmerge_continue = r.u8()? != 0;
                            let text = r.string()?;
                            cells.push(Cell {
                                grid_col,
                                grid_span,
                                vmerge_continue,
                                text,
                            });
                        }
                        rows.push(Row { cells });
                    }
                    blocks.push(Block::Table(Table { index, rows }));
                }
                _ => return Err(corrupt("bad block tag")),
            }
        }
        let nh = r.u32()?;
        let mut hyperlinks = Vec::new();
        for _ in 0..nh {
            let text = r.string()?;
            let rel_id = r.opt_string()?;
            let anchor = r.opt_string()?;
            hyperlinks.push(Hyperlink {
                text,
                rel_id,
                anchor,
            });
        }
        let nbm = r.u32()?;
        let mut bookmarks = Vec::new();
        for _ in 0..nbm {
            bookmarks.push(r.string()?);
        }
        let nres = r.u32()?;
        let mut resources = Vec::new();
        for _ in 0..nres {
            resources.push(r.string()?);
        }
        let section_count = r.u32()?;
        let nf = r.u32()?;
        let mut fields = Vec::new();
        for _ in 0..nf {
            fields.push(r.string()?);
        }
        if !r.at_end() {
            return Err(corrupt("story model has trailing bytes"));
        }
        Ok(StoryModel {
            story,
            part_name,
            root_local,
            blocks,
            hyperlinks,
            bookmarks,
            resources,
            section_count,
            fields,
        })
    }
}

fn story_index(story: DocxStory) -> u32 {
    match story {
        DocxStory::Main => 0,
        DocxStory::Header(n) | DocxStory::Footer(n) | DocxStory::TextBox(n) => n,
        DocxStory::Footnote(id) | DocxStory::Endnote(id) | DocxStory::Comment(id) => id,
    }
}

fn story_from_tag(tag: u8, index: u32) -> Result<DocxStory> {
    Ok(match tag {
        0 => DocxStory::Main,
        1 => DocxStory::Header(index),
        2 => DocxStory::Footer(index),
        3 => DocxStory::Footnote(index),
        4 => DocxStory::Endnote(index),
        5 => DocxStory::Comment(index),
        6 => DocxStory::TextBox(index),
        _ => return Err(corrupt("bad story tag")),
    })
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

/// Parse a `styles.xml` part into a resolved [`StyleTable`].
pub fn parse_styles(bytes: &[u8], limits: Limits) -> Result<StyleTable> {
    harden(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut raw: BTreeMap<String, (Option<String>, Option<u8>)> = BTreeMap::new();
    let mut cur: Option<(String, Option<String>, Option<u8>)> = None;
    let mut st = Budget::new();
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(doctype_declined()),
            Event::Start(e) => {
                st.open(limits)?;
                handle_style_element(&e, limits, &mut raw, &mut cur)?;
            }
            Event::Empty(e) => {
                st.open(limits)?;
                st.close();
                handle_style_element(&e, limits, &mut raw, &mut cur)?;
            }
            Event::End(e) => {
                st.close();
                if local_name_bytes(e.name().as_ref()) == b"style"
                    && let Some((id, based, ol)) = cur.take()
                {
                    raw.insert(id, (based, ol));
                }
            }
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    Ok(resolve_styles(&raw))
}

fn handle_style_element(
    e: &BytesStart<'_>,
    limits: Limits,
    raw: &mut BTreeMap<String, (Option<String>, Option<u8>)>,
    cur: &mut Option<(String, Option<String>, Option<u8>)>,
) -> Result<()> {
    let _ = raw;
    let local = local_name(e);
    match local.as_str() {
        "style" => {
            let attrs = read_attrs(e, limits)?;
            if let Some(id) = attr_of(&attrs, "styleId") {
                *cur = Some((id.to_string(), None, None));
            }
        }
        "basedOn" => {
            if let Some(c) = cur.as_mut() {
                let attrs = read_attrs(e, limits)?;
                if let Some(v) = attr_of(&attrs, "val") {
                    c.1 = Some(v.to_string());
                }
            }
        }
        "outlineLvl" => {
            if let Some(c) = cur.as_mut() {
                let attrs = read_attrs(e, limits)?;
                if let Some(v) = attr_of(&attrs, "val")
                    && let Ok(n) = v.parse::<u8>()
                {
                    c.2 = Some(n);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn resolve_styles(raw: &BTreeMap<String, (Option<String>, Option<u8>)>) -> StyleTable {
    let mut pairs: Vec<(String, u8)> = Vec::new();
    for (id, (based, ol)) in raw {
        if let Some(v) = ol {
            pairs.push((id.clone(), *v));
            continue;
        }
        let mut cursor = based.clone();
        let mut depth = 0;
        while let Some(b) = cursor {
            if depth > 16 {
                break;
            }
            match raw.get(&b) {
                Some((next, next_ol)) => {
                    if let Some(v) = next_ol {
                        pairs.push((id.clone(), *v));
                        break;
                    }
                    cursor = next.clone();
                }
                None => break,
            }
            depth += 1;
        }
    }
    StyleTable::from_outline_pairs(pairs)
}

// ---------------------------------------------------------------------------
// Story parsing
// ---------------------------------------------------------------------------

/// Parse one story part into a canonical [`StoryModel`], honoring `profile`.
///
/// `styles` resolves heading identity; when absent, `heading_level` is `None`.
pub fn parse_story(
    part_bytes: &[u8],
    part_name: &str,
    story: DocxStory,
    profile: &DocxExtractProfile,
    styles: Option<&StyleTable>,
    limits: Limits,
) -> Result<StoryModel> {
    harden(part_bytes, limits)?;
    let mut reader = Reader::from_reader(part_bytes);
    reader.config_mut().trim_text(false);
    let mut p = Parser::new(story, profile, styles);
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        p.budget.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(doctype_declined()),
            Event::Start(e) => {
                if !saw_root {
                    p.check_root(&e)?;
                    saw_root = true;
                }
                p.on_open(&e, limits, false)?;
            }
            Event::Empty(e) => {
                if !saw_root {
                    p.check_root(&e)?;
                    saw_root = true;
                }
                p.on_open(&e, limits, true)?;
            }
            Event::End(e) => {
                let name = e.name();
                let local = local_name_bytes(name.as_ref());
                p.on_close(local)?;
            }
            Event::Text(t) => {
                let s = t.into_inner();
                p.text(s.as_ref(), limits)?;
            }
            Event::GeneralRef(r) => {
                let s = if r.is_char_ref() {
                    match r.resolve_char_ref() {
                        Ok(Some(c)) => c.to_string(),
                        _ => String::new(),
                    }
                } else {
                    match r.into_inner().as_ref() {
                        "amp" => "&".to_string(),
                        "lt" => "<".to_string(),
                        "gt" => ">".to_string(),
                        "quot" => "\"".to_string(),
                        "apos" => "'".to_string(),
                        // No DTD/entity resolver: an undeclared entity is not
                        // expanded (and never resolved from a network or file).
                        _ => String::new(),
                    }
                };
                p.text(s.as_ref(), limits)?;
            }
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("story XML part is empty"));
    }
    if p.stack.len() != 1 {
        return Err(Error::invalid_xml_structure(
            "story XML part is not well-formed (unclosed elements)",
        ));
    }
    Ok(p.finish(part_name))
}

enum Cont {
    Body,
    Table { table: TableBuilder, nested: bool },
    Row { row: RowBuilder },
    Cell { cell: CellBuilder },
    Para { para: ParaBuilder },
    Note,
}

struct ParaBuilder {
    text: String,
    style_id: Option<String>,
    runs: Vec<Run>,
}

struct CellBuilder {
    grid_span: u32,
    vmerge_restart: bool,
    vmerge_present: bool,
    paragraphs: Vec<String>,
    raw_text: String,
}

struct RowBuilder {
    cells: Vec<Cell>,
    col_cursor: u32,
}

struct TableBuilder {
    rows: Vec<Row>,
}

struct HyperlinkBuilder {
    text: String,
    rel_id: Option<String>,
    anchor: Option<String>,
}

struct Parser<'a> {
    story: DocxStory,
    profile: &'a DocxExtractProfile,
    styles: Option<&'a StyleTable>,
    stack: Vec<Cont>,
    blocks: Vec<Block>,
    hyperlinks: Vec<Hyperlink>,
    hyper_stack: Vec<HyperlinkBuilder>,
    bookmarks: Vec<String>,
    resources: Vec<String>,
    fields: Vec<String>,
    section_count: u32,
    para_counter: u32,
    table_counter: u32,
    ins_depth: u32,
    del_depth: u32,
    in_t: u32,
    in_deltext: u32,
    in_instr: u32,
    in_run: bool,
    run_text: String,
    run_style: Option<String>,
    run_hidden: bool,
    suppress_result: bool,
    note_ok: bool,
    budget: Budget,
}

struct Budget {
    events: u64,
    nodes: u64,
    depth: u64,
    text: u64,
}

impl Budget {
    fn new() -> Self {
        Budget {
            events: 0,
            nodes: 0,
            depth: 0,
            text: 0,
        }
    }
    fn event(&mut self, limits: Limits) -> Result<()> {
        self.events = self.events.saturating_add(1);
        if self.events > limits.max_xml_events {
            return Err(Error::resource_limit("XML event bound exceeded"));
        }
        Ok(())
    }
    fn open(&mut self, limits: Limits) -> Result<()> {
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
    fn close(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
    fn text(&mut self, n: usize, limits: Limits) -> Result<()> {
        self.text = self.text.saturating_add(n as u64);
        if self.text > limits.max_xml_text_bytes {
            return Err(Error::resource_limit("XML text bound exceeded"));
        }
        Ok(())
    }
}

impl<'a> Parser<'a> {
    fn new(
        story: DocxStory,
        profile: &'a DocxExtractProfile,
        styles: Option<&'a StyleTable>,
    ) -> Self {
        Parser {
            story,
            profile,
            styles,
            stack: vec![Cont::Body],
            blocks: Vec::new(),
            hyperlinks: Vec::new(),
            hyper_stack: Vec::new(),
            bookmarks: Vec::new(),
            resources: Vec::new(),
            fields: Vec::new(),
            section_count: 0,
            para_counter: 0,
            table_counter: 0,
            ins_depth: 0,
            del_depth: 0,
            in_t: 0,
            in_deltext: 0,
            in_instr: 0,
            in_run: false,
            run_text: String::new(),
            run_style: None,
            run_hidden: false,
            suppress_result: false,
            note_ok: true,
            budget: Budget::new(),
        }
    }

    fn check_root(&self, e: &BytesStart<'_>) -> Result<()> {
        let local = local_name(e);
        if local != self.story.expected_root() {
            return Err(Error::invalid_package_structure(format!(
                "story {}: root element <{local}> is not <{}>",
                self.story.name(),
                self.story.expected_root()
            )));
        }
        Ok(())
    }

    fn t_included(&self) -> bool {
        match self.profile.tracked {
            super::TrackedChanges::Final | super::TrackedChanges::All => true,
            super::TrackedChanges::Original => self.ins_depth == 0,
        }
    }

    fn deltext_included(&self) -> bool {
        matches!(
            self.profile.tracked,
            super::TrackedChanges::Original | super::TrackedChanges::All
        )
    }

    fn instr_included(&self) -> bool {
        matches!(self.profile.fields, FieldMode::Code | FieldMode::Both)
    }

    fn text_ok(&self) -> bool {
        self.note_ok && !(self.run_hidden && !self.profile.hidden)
    }

    fn text(&mut self, s: &str, limits: Limits) -> Result<()> {
        self.budget.text(s.len(), limits)?;
        if self.in_deltext > 0 {
            if self.deltext_included() && self.text_ok() {
                self.push_text(s);
            }
        } else if self.in_instr > 0 {
            if self.instr_included() && self.note_ok {
                self.push_text(s);
            }
        } else if self.in_t > 0 && self.t_included() && !self.suppress_result && self.text_ok() {
            self.push_text(s);
        }
        Ok(())
    }

    fn push_text(&mut self, s: &str) {
        if !self.note_ok {
            return;
        }
        if self.in_run && self.stack.iter().any(|c| matches!(c, Cont::Para { .. })) {
            self.run_text.push_str(s);
            return;
        }
        for c in self.stack.iter_mut().rev() {
            match c {
                Cont::Para { para } => {
                    para.text.push_str(s);
                    return;
                }
                Cont::Cell { cell } => {
                    cell.raw_text.push_str(s);
                    return;
                }
                _ => {}
            }
        }
    }

    fn on_open(&mut self, e: &BytesStart<'_>, limits: Limits, empty: bool) -> Result<()> {
        self.budget.open(limits)?;
        let local = local_name(e);
        let attrs = read_attrs(e, limits)?;
        match local.as_str() {
            "footnote" | "endnote" | "comment" => {
                let id = attr_of(&attrs, "id")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
                let matches = self.story.note_id() == Some(id);
                self.note_ok = matches;
                self.stack.push(Cont::Note);
            }
            "p" => self.stack.push(Cont::Para {
                para: ParaBuilder {
                    text: String::new(),
                    style_id: None,
                    runs: Vec::new(),
                },
            }),
            "tbl" => {
                let nested = matches!(self.stack.last(), Some(Cont::Cell { .. }));
                self.stack.push(Cont::Table {
                    table: TableBuilder { rows: Vec::new() },
                    nested,
                });
            }
            "tr" => self.stack.push(Cont::Row {
                row: RowBuilder {
                    cells: Vec::new(),
                    col_cursor: 0,
                },
            }),
            "tc" => self.stack.push(Cont::Cell {
                cell: CellBuilder {
                    grid_span: 1,
                    vmerge_restart: false,
                    vmerge_present: false,
                    paragraphs: Vec::new(),
                    raw_text: String::new(),
                },
            }),
            "gridSpan" => {
                if let Some(v) = attr_of(&attrs, "val")
                    && let Ok(n) = v.parse::<u32>()
                    && let Some(Cont::Cell { cell }) = self.stack.last_mut()
                {
                    cell.grid_span = n.max(1);
                }
            }
            "vMerge" => {
                if let Some(Cont::Cell { cell }) = self.stack.last_mut() {
                    cell.vmerge_present = true;
                    cell.vmerge_restart = attr_of(&attrs, "val") == Some("restart");
                }
            }
            "pStyle" => {
                if let Some(v) = attr_of(&attrs, "val")
                    && let Some(Cont::Para { para }) = self.stack.last_mut()
                {
                    para.style_id = Some(v.to_string());
                }
            }
            "rStyle" => {
                if let Some(v) = attr_of(&attrs, "val") {
                    self.run_style = Some(v.to_string());
                }
            }
            "r" => {
                self.in_run = true;
                self.run_text.clear();
                self.run_style = None;
                self.run_hidden = false;
            }
            "t" => self.in_t += 1,
            "delText" => self.in_deltext += 1,
            "instrText" => self.in_instr += 1,
            "vanish" => {
                let hidden = attr_of(&attrs, "val") != Some("false");
                if hidden {
                    self.run_hidden = true;
                }
            }
            "tab" => {
                if self.profile.tabs {
                    self.push_text("\t");
                }
            }
            "br" | "cr" => {
                if self.profile.breaks {
                    self.push_text("\n");
                }
            }
            "ins" => self.ins_depth += 1,
            "del" => self.del_depth += 1,
            "sectPr" => self.section_count += 1,
            "bookmarkStart" => {
                if let Some(n) = attr_of(&attrs, "name") {
                    self.bookmarks.push(n.to_string());
                }
            }
            "hyperlink" => self.hyper_stack.push(HyperlinkBuilder {
                text: String::new(),
                rel_id: attr_of(&attrs, "id").map(str::to_string),
                anchor: attr_of(&attrs, "anchor").map(str::to_string),
            }),
            "blip" | "imagedata" | "oleObject" => {
                if let Some(id) = attr_of(&attrs, "embed").or_else(|| attr_of(&attrs, "id")) {
                    self.resources.push(id.to_string());
                }
            }
            "fldSimple" => {
                if let Some(instr) = attr_of(&attrs, "instr") {
                    self.fields.push(instr.to_string());
                    if self.profile.fields == FieldMode::Code {
                        self.suppress_result = true;
                    }
                }
            }
            _ => {}
        }
        if empty {
            self.on_close(local.as_bytes())?;
        }
        Ok(())
    }

    fn on_close(&mut self, local: &[u8]) -> Result<()> {
        match local {
            b"footnote" | b"endnote" | b"comment" => {
                self.stack.pop();
                self.note_ok = true;
            }
            b"p" => self.close_para(),
            b"tbl" => self.close_table(),
            b"tr" => {
                if let Some(Cont::Row { row }) = self.stack.pop()
                    && let Some(Cont::Table { table, .. }) = self.stack.last_mut()
                {
                    table.rows.push(Row { cells: row.cells });
                }
            }
            b"tc" => {
                if let Some(Cont::Cell { cell }) = self.stack.pop()
                    && let Some(Cont::Row { row }) = self.stack.last_mut()
                {
                    let text = if cell.paragraphs.is_empty() {
                        cell.raw_text
                    } else {
                        cell.paragraphs.join("\n")
                    };
                    let vmerge_continue = cell.vmerge_present && !cell.vmerge_restart;
                    let grid_span = cell.grid_span.max(1);
                    row.cells.push(Cell {
                        grid_col: row.col_cursor,
                        grid_span,
                        vmerge_continue,
                        text,
                    });
                    row.col_cursor = row.col_cursor.saturating_add(grid_span);
                }
            }
            b"r" => {
                self.in_run = false;
                let text = core::mem::take(&mut self.run_text);
                let style = self.run_style.take();
                if (!text.is_empty() || style.is_some())
                    && let Some(Cont::Para { para }) = self
                        .stack
                        .iter_mut()
                        .rev()
                        .find(|c| matches!(c, Cont::Para { .. }))
                {
                    let index = para.runs.len() as u32;
                    para.text.push_str(&text);
                    para.runs.push(Run {
                        index,
                        text: text.clone(),
                        style_id: style,
                    });
                }
                if let Some(h) = self.hyper_stack.last_mut() {
                    h.text.push_str(&text);
                }
            }
            b"t" => self.in_t = self.in_t.saturating_sub(1),
            b"delText" => self.in_deltext = self.in_deltext.saturating_sub(1),
            b"instrText" => self.in_instr = self.in_instr.saturating_sub(1),
            b"ins" => self.ins_depth = self.ins_depth.saturating_sub(1),
            b"del" => self.del_depth = self.del_depth.saturating_sub(1),
            b"fldSimple" => self.suppress_result = false,
            b"hyperlink" => {
                if let Some(h) = self.hyper_stack.pop() {
                    self.hyperlinks.push(Hyperlink {
                        text: h.text,
                        rel_id: h.rel_id,
                        anchor: h.anchor,
                    });
                }
            }
            _ => {}
        }
        self.budget.close();
        Ok(())
    }

    fn close_para(&mut self) {
        let Some(Cont::Para { para }) = self.stack.pop() else {
            return;
        };
        if !self.note_ok {
            return;
        }
        let heading_level = para
            .style_id
            .as_deref()
            .and_then(|s| self.styles.and_then(|t| t.outline_level(s)));
        if matches!(self.stack.last(), Some(Cont::Cell { .. })) {
            if let Some(Cont::Cell { cell }) = self.stack.last_mut() {
                cell.paragraphs.push(para.text);
            }
            return;
        }
        let index = self.para_counter;
        self.para_counter += 1;
        self.blocks.push(Block::Paragraph(Paragraph {
            index,
            text: para.text,
            style_id: para.style_id,
            heading_level,
            runs: para.runs,
        }));
    }

    fn close_table(&mut self) {
        let Some(Cont::Table { table, nested }) = self.stack.pop() else {
            return;
        };
        if !self.note_ok {
            return;
        }
        let index = self.table_counter;
        if nested {
            let text = Table {
                index,
                rows: table.rows,
            }
            .text();
            self.push_text(&text);
        } else {
            self.table_counter += 1;
            self.blocks.push(Block::Table(Table {
                index,
                rows: table.rows,
            }));
        }
    }

    fn finish(mut self, part_name: &str) -> StoryModel {
        if self.in_run {
            // An unterminated run cannot happen for well-formed XML, but flush
            // deterministically rather than dropping text.
            let text = core::mem::take(&mut self.run_text);
            if let Some(h) = self.hyper_stack.last_mut() {
                h.text.push_str(&text);
            }
        }
        StoryModel {
            story: self.story,
            part_name: part_name.to_string(),
            root_local: self.story.expected_root().to_string(),
            blocks: self.blocks,
            hyperlinks: self.hyperlinks,
            bookmarks: self.bookmarks,
            resources: self.resources,
            section_count: self.section_count,
            fields: self.fields,
        }
    }
}

// ---------------------------------------------------------------------------
// XML helpers
// ---------------------------------------------------------------------------

fn harden(bytes: &[u8], limits: Limits) -> Result<()> {
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

fn xml_err(e: quick_xml::Error) -> Error {
    Error::invalid_xml_structure(format!("malformed XML: {e}"))
}

fn doctype_declined() -> Error {
    Error::invalid_xml_structure("DOCTYPE is forbidden in a WordprocessingML part")
}

fn local_name(e: &BytesStart<'_>) -> String {
    e.name().local_name().as_ref().to_string()
}

fn local_name_bytes(qname: &str) -> &[u8] {
    match qname.rfind(':') {
        Some(i) => &qname.as_bytes()[i + 1..],
        None => qname.as_bytes(),
    }
}

fn read_attrs(e: &BytesStart<'_>, limits: Limits) -> Result<Vec<(String, String)>> {
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

fn attr_of<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_package_structure(format!("corrupt DOCX story model: {msg}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::docx::{DocxExtractProfile, TrackedChanges};

    fn parse(xml: &str) -> StoryModel {
        parse_story(
            xml.as_bytes(),
            "/word/document.xml",
            DocxStory::Main,
            &DocxExtractProfile::DEFAULT,
            None,
            Limits::DEFAULT,
        )
        .unwrap()
    }

    #[test]
    fn paragraphs_and_runs() {
        let m = parse(
            r#"<w:document xmlns:w="x"><w:body>
              <w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> world</w:t></w:r></w:p>
              <w:p><w:r><w:t>Second</w:t></w:r></w:p>
            </w:body></w:document>"#,
        );
        assert_eq!(m.text(), "Hello world\nSecond");
        assert_eq!(m.paragraphs().count(), 2);
    }

    #[test]
    fn tracked_changes_final_vs_original() {
        let xml = r#"<w:document xmlns:w="x"><w:body>
          <w:p><w:r><w:t>base </w:t></w:r><w:ins><w:r><w:t>added</w:t></w:r></w:ins><w:del><w:r><w:delText>gone</w:delText></w:r></w:del></w:p>
        </w:body></w:document>"#;
        let final_model = parse_story(
            xml.as_bytes(),
            "/w",
            DocxStory::Main,
            &DocxExtractProfile::DEFAULT,
            None,
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(final_model.text(), "base added");
        let mut orig_profile = DocxExtractProfile::DEFAULT;
        orig_profile.tracked = TrackedChanges::Original;
        let orig = parse_story(
            xml.as_bytes(),
            "/w",
            DocxStory::Main,
            &orig_profile,
            None,
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(orig.text(), "base gone");
    }

    #[test]
    fn table_grid_and_vmerge() {
        let xml = r#"<w:document xmlns:w="x"><w:body><w:tbl>
          <w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>A1</w:t></w:r></w:p></w:tc></w:tr>
          <w:tr><w:tc><w:p><w:r><w:t>B1</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p><w:r><w:t>B2</w:t></w:r></w:p></w:tc></w:tr>
        </w:tbl></w:body></w:document>"#;
        let m = parse(xml);
        let t = m.tables().next().unwrap();
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0].cells[0].grid_span, 2);
        assert!(t.rows[1].cells[1].vmerge_continue);
        assert_eq!(t.rows[1].cells[1].grid_col, 1);
    }

    #[test]
    fn wrong_root_declines() {
        let e = parse_story(
            b"<w:foo xmlns:w=\"x\"/>",
            "/w",
            DocxStory::Main,
            &DocxExtractProfile::DEFAULT,
            None,
            Limits::DEFAULT,
        )
        .unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidPackageStructure);
    }

    #[test]
    fn doctype_declines() {
        let e = parse_story(
            b"<!DOCTYPE x><w:document xmlns:w=\"y\"/>",
            "/w",
            DocxStory::Main,
            &DocxExtractProfile::DEFAULT,
            None,
            Limits::DEFAULT,
        )
        .unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidXmlStructure);
    }

    #[test]
    fn note_filter_selects_one_note() {
        let xml = r#"<w:footnotes xmlns:w="x">
          <w:footnote w:id="1"><w:p><w:r><w:t>one</w:t></w:r></w:p></w:footnote>
          <w:footnote w:id="2"><w:p><w:r><w:t>two</w:t></w:r></w:p></w:footnote>
        </w:footnotes>"#;
        let m = parse_story(
            xml.as_bytes(),
            "/word/footnotes.xml",
            DocxStory::Footnote(2),
            &DocxExtractProfile::DEFAULT,
            None,
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(m.text(), "two");
    }
}
