//! Bounded, representation-preserving KML / GPX geospatial adapter (Phase 21.23).
//!
//! KML and GPX are both XML, so the physical bytes are shared with the standalone
//! [`XML adapter`](crate::adapter::xml). This adapter therefore **reuses the shared
//! bounded XML parser and policy** ([`crate::adapter::xml`]) to obtain a
//! span-preserving element/attribute tree, and layers a small,
//! representation-preserving *geospatial* projection on top. The exact authority is
//! still the whole source (a `DocumentExact`, a RAW-like authority): everything this
//! module produces is a bounded, deterministic (`Q_gen`) projection that never sits
//! on the exactness path.
//!
//! ## Why a separate dialect
//!
//! Two geospatial documents that are byte-for-byte different can share every XML
//! construct, so the *semantic* dialect matters: KML (OGC KML 2.2) is
//! `<kml xmlns="http://www.opengis.net/kml/2.2">` with `<Document>`/`<Folder>`/
//! `<Placemark>` features and a `<Point>`/`<LineString>`/`<Polygon>` geometry whose
//! `<coordinates>` holds the numbers; GPX (GPX 1.1) is
//! `<gpx xmlns="http://www.topografix.com/GPX/1/1">` with `<metadata>`, `<wpt>`
//! waypoints, `<rte>`/`<rtept>` routes, and `<trk>`/`<trkseg>`/`<trkpt>` tracks
//! (whose `lat`/`lon` live in attributes and whose `<ele>`/`<time>` are child
//! elements). The dialect is recorded in the model exactly as the CSV/TSV adapter
//! records its delimiter/terminator dialect, the config adapter records its
//! INI/`.env`/`properties` dialect, and the feed adapter records its `rss`/`atom`
//! dialect.
//!
//! ## Representation preservation
//!
//! Because the model embeds the full [`XmlModel`], it preserves, for the whole
//! document and for every recognized field:
//!
//! * every element's qualified name, start-tag, end-tag, and full **byte span**;
//! * every attribute's name/quoted-value/inner-value/full span, including the GPX
//!   point `<trkpt lat="…" lon="…">` spelling, in source order;
//! * every recognized element's **document order** (the record/point arenas are
//!   ordered);
//! * the **namespace declaration** on the KML (`xmlns="…/kml/2.2"`) or GPX
//!   (`xmlns="…/GPX/1/1"`) root (kept as an ordinary attribute with its exact span);
//! * entity references are **never expanded** and no DTD internal subset is
//!   processed (the shared XML policy declines it).
//!
//! ## Detection (semantic sub-detection — the critical part)
//!
//! The physical bytes are XML, so a geospatial document must be distinguished from a
//! plain XML document by a **bounded semantic test** run *before* the generic XML
//! detector:
//!
//! * **KML** — the root element is `<kml>` whose namespace prefix is bound to the
//!   KML namespace URI `http://www.opengis.net/kml/2.2`, and the root has at least
//!   one recognized structural child (`Document`, `Folder`, or `Placemark`);
//! * **GPX** — the root element is `<gpx>` whose namespace prefix is bound to the
//!   GPX namespace URI `http://www.topografix.com/GPX/1/1`, and the root has at
//!   least one recognized structural child (`metadata`, `wpt`, `rte`, or `trk`).
//!
//! Everything else declines: a plain XML document stays
//! [`Xml`](crate::field::document_format::DocumentFormat::Xml), an HTML document
//! (already claimed earlier) stays `Html`, and a `<kml>`/`<gpx>`-shaped-but-invalid
//! document (no namespace, or no recognized structural child), and a non-geospatial
//! root all decline and stay on the XML/opaque path.
//!
//! ### What the detector cannot distinguish (recorded honestly)
//!
//! * **Older / other KML namespace versions** — KML 2.0 (`…/kml/2.0`) and KML 2.1
//!   (`…/kml/2.1`), and the pre-OGC Google Earth namespaces, are **not** the OGC
//!   KML 2.2 URI, so they stay `Xml`. Only the KML 2.2 URI is claimed.
//! * **GPX 1.0** — its namespace URI is `http://www.topografix.com/GPX/1/0`, not the
//!   GPX 1.1 URI, so it stays `Xml`. Only the GPX 1.1 URI is claimed.
//! * **A namespaced KML/GPX in a non-default prefix** — e.g. `<k:kml
//!   xmlns:k="…/kml/2.2">` — **is** recognized (the bound prefix is read, not the
//!   default one), exactly as for Atom in the feed adapter.
//! * **A KML/GPX document with no structural child** — a `<kml>` with no
//!   `Document`/`Folder`/`Placemark`, or a `<gpx>` with no
//!   `metadata`/`wpt`/`rte`/`trk`, declines (the structural-child test is the
//!   signal), so an empty-but-well-formed root is not claimed.
//! * **Geometry coordinate grammar** — the adapter does **not** validate the KML
//!   `<coordinates>` or GPX `lat`/`lon` positional grammar; it preserves whatever
//!   is present verbatim.
//!
//! ## Bounds
//!
//! Parsing reuses the shared bounded XML parser (depth/nodes/events/text caps,
//! NaN-free, no `unwrap`/`panic`) and additionally charges the recognized record and
//! point arenas against [`Limits::max_gis_placemarks`], [`Limits::max_gis_tracks`],
//! and [`Limits::max_gis_points`], the aggregate recognized-field bytes against
//! [`Limits::max_gis_text_bytes`], the depth against [`Limits::max_gis_depth`], the
//! embedded node count against [`Limits::max_gis_nodes`], and the source against
//! [`Limits::max_gis_document_bytes`]. Untrusted input can only ever yield a typed
//! decline.

use crate::adapter::xml::{
    K_ELEMENT, XNode, XmlModel, attr_name, attr_value_bytes, element_name, parse as xml_parse,
    subtree_text, token_bytes,
};
use crate::error::{Error, ErrorClass, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model entries (defends the decoder against a hostile blob).
pub const MAX_MODEL_ENTRIES: u32 = 1 << 20;

/// Dialect tag: OGC KML 2.2 (`<kml xmlns="…/kml/2.2">…<Placemark>…`).
pub const DIALECT_KML: u8 = 0;
/// Dialect tag: GPX 1.1 (`<gpx xmlns="…/GPX/1/1">…<trkpt>…`).
pub const DIALECT_GPX: u8 = 1;

/// The OGC KML 2.2 namespace URI.
pub const KML_NS: &str = "http://www.opengis.net/kml/2.2";
/// The GPX 1.1 namespace URI.
pub const GPX_NS: &str = "http://www.topografix.com/GPX/1/1";

/// Stable name for a dialect tag.
pub const fn dialect_name(d: u8) -> &'static str {
    match d {
        DIALECT_GPX => "gpx",
        _ => "kml",
    }
}

/// The recognized root-level field element local names, by dialect.
///
/// These are the root's **scalar metadata** children, not the structural
/// containers: the KML `<Document>`/`<Folder>`/`<Placemark>` features and the GPX
/// `<metadata>`/`<wpt>`/`<rte>`/`<trk>` records are enumerated by the record/point
/// arenas instead, so a container's whole-subtree text is never counted twice.
pub const fn root_fields(dialect: u8) -> &'static [&'static str] {
    match dialect {
        DIALECT_GPX => &["name", "desc", "author", "time", "keywords"],
        _ => &["name", "description"],
    }
}

/// The recognized record-level field element local names, by dialect. For KML a
/// record is a `<Placemark>`; for GPX a record is a `<wpt>`/`<rte>`/`<trk>`. The
/// structural sub-containers (a Placemark's geometry, a `<trk>`'s `<trkseg>`, a
/// `<rte>`'s `<rtept>`) are enumerated by the point arena instead.
pub const fn record_fields(dialect: u8) -> &'static [&'static str] {
    match dialect {
        DIALECT_GPX => &[
            "name", "cmt", "desc", "src", "link", "number", "type", "sym", "ele", "time",
        ],
        _ => &[
            "name",
            "description",
            "visibility",
            "styleUrl",
            "Style",
            "TimeStamp",
            "TimeSpan",
            "ExtendedData",
        ],
    }
}

/// The recognized point-level field element local names, by dialect. For KML a point
/// is a geometry element (a `Point`/`LineString`/`Polygon`, whose `coordinates` is
/// the recognized field); for GPX a point is a `<wpt>`/`<rtept>`/`<trkpt>` element.
pub const fn point_fields(dialect: u8) -> &'static [&'static str] {
    match dialect {
        DIALECT_GPX => &["ele", "time", "name", "cmt", "desc", "sym", "type", "src"],
        _ => &["coordinates"],
    }
}

/// The KML structural container local names (a root `<kml>` must have one).
pub const KML_CONTAINERS: &[&str] = &["Document", "Folder", "Placemark"];
/// The KML geometry local names (the point arena).
pub const KML_GEOMETRIES: &[&str] = &["Point", "LineString", "Polygon"];
/// The GPX record local names (a root `<gpx>` must have one).
pub const GPX_RECORDS: &[&str] = &["wpt", "rte", "trk"];
/// The GPX structural child local names (a root `<gpx>` must have one).
pub const GPX_CHILDREN: &[&str] = &["metadata", "wpt", "rte", "trk"];
/// The GPX point local names (the point arena).
pub const GPX_POINTS: &[&str] = &["wpt", "rtept", "trkpt"];

/// The part of a qualified name after the last `:`, with its prefix.
pub fn split_qname(name: &str) -> (&str, &str) {
    match name.rsplit_once(':') {
        Some((prefix, local)) => (prefix, local),
        None => ("", name),
    }
}

/// One attribute of a field element: its qualified name (spelling preserved), its
/// decoded value, and its exact span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAttr {
    /// The qualified attribute name (prefix intact).
    pub name: String,
    /// The decoded attribute value.
    pub value: String,
    /// The exact span of the whole attribute.
    pub start: u64,
    /// One past the attribute.
    pub end: u64,
}

/// One lexical match from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GisMatch {
    /// The arena the matching field belongs to (`"root"`, `"record"`, or `"point"`).
    pub kind: &'static str,
    /// The 0-based record/point ordinal, or `None` for a root-level field.
    pub index: Option<u32>,
    /// The matched field's local name.
    pub name: String,
    /// The exact source span of the matching field element.
    pub start: u64,
    /// One past the matching field element.
    pub end: u64,
    /// The decoded field text.
    pub text: String,
}

/// The canonical derived geospatial model (the materialization of a `GisModel`
/// node).
///
/// The embedded [`XmlModel`] carries the whole span-preserving tree; `root`,
/// `records`, and `points` are indices into that tree. For KML, `records` are the
/// `<Placemark>` features (all of them, in document order) and `points` are the
/// geometry elements (`Point`/`LineString`/`Polygon`). For GPX, `records` are the
/// top-level `<wpt>`/`<rte>`/`<trk>` records and `points` are the `<wpt>`/`<rtept>`/
/// `<trkpt>` point elements in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GisModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The recorded dialect (`DIALECT_*`).
    pub dialect: u8,
    /// The XML root element node index (`<kml>` or `<gpx>`).
    pub root: u32,
    /// The record element node indices, in document order.
    pub records: Vec<u32>,
    /// The point element node indices, in document order.
    pub points: Vec<u32>,
    /// The span-preserving XML tree the indices refer to.
    pub xml: XmlModel,
}

impl GisModel {
    /// The XML node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&XNode> {
        self.xml.node(index)
    }

    /// The number of records in the model.
    pub fn record_count(&self) -> u32 {
        self.records.len() as u32
    }

    /// The number of points in the model.
    pub fn point_count(&self) -> u32 {
        self.points.len() as u32
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let xml_bytes = self.xml.encode();
        let mut out =
            Vec::with_capacity(40 + xml_bytes.len() + (self.records.len() + self.points.len()) * 4);
        out.extend_from_slice(b"GISM");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        out.push(0); // reserved
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            out.extend_from_slice(&r.to_le_bytes());
        }
        out.extend_from_slice(&(self.points.len() as u32).to_le_bytes());
        for p in &self.points {
            out.extend_from_slice(&p.to_le_bytes());
        }
        out.extend_from_slice(&(xml_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&xml_bytes);
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<GisModel> {
        let mut r = Reader::new(bytes);
        if r.bytes(4)? != b"GISM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > DIALECT_GPX {
            return Err(corrupt("unknown dialect"));
        }
        if r.u8()? != 0 {
            return Err(corrupt("unknown model flags"));
        }
        let doc_len = r.u64()?;
        let root = r.u32()?;
        let records = read_indices(&mut r)?;
        let points = read_indices(&mut r)?;
        let xml_len = r.u32()?;
        let xml_bytes = r.bytes(xml_len as usize)?;
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        let xml = XmlModel::decode(xml_bytes)?;
        if xml.doc_len != doc_len {
            return Err(corrupt("model doc_len disagrees with the XML model"));
        }
        let nnodes = xml.nodes.len() as u32;
        if root >= nnodes {
            return Err(corrupt("root index is out of range"));
        }
        for &n in records.iter().chain(points.iter()) {
            if n >= nnodes {
                return Err(corrupt("record/point index is out of range"));
            }
        }
        Ok(GisModel {
            doc_len,
            dialect,
            root,
            records,
            points,
            xml,
        })
    }
}

/// Read a length-prefixed index vector, bounded by [`MAX_MODEL_ENTRIES`].
fn read_indices(r: &mut Reader<'_>) -> Result<Vec<u32>> {
    let count = r.u32()?;
    if count > MAX_MODEL_ENTRIES {
        return Err(corrupt("model index count is implausible"));
    }
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        out.push(r.u32()?);
    }
    Ok(out)
}

/// Byte-based, conservative KML/GPX detector. See the module docs for the exact
/// semantic boundary.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    classify(source, limits).is_ok()
}

/// Classify `source` as one geospatial dialect, or decline typed when it is not a
/// KML 2.2 / GPX 1.1 document under the semantic test.
pub fn classify(source: &[u8], limits: Limits) -> Result<u8> {
    Ok(parse(source, limits, false)?.dialect)
}

/// Parse `source` into a [`GisModel`]. `build` selects whether the record/point
/// arenas are populated (detection runs with `build = false`); the XML tree is always
/// built because the semantic test walks it.
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<GisModel> {
    if source.len() as u64 > limits.max_gis_document_bytes {
        return Err(Error::resource_limit(format!(
            "geospatial source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_gis_document_bytes
        )));
    }
    let xml = xml_parse(source, limits, true).map_err(|e| match e.class() {
        ErrorClass::ResourceLimit => e,
        _ => invalid_gis(format!(
            "geospatial source is not well-formed XML: {}",
            e.message()
        )),
    })?;
    if xml.nodes.len() as u64 > u64::from(limits.max_gis_nodes) {
        return Err(Error::resource_limit(format!(
            "geospatial document exceeds the {}-node cap",
            limits.max_gis_nodes
        )));
    }
    let (dialect, records_all, points_all) = analyze(&xml, source, limits)?;
    Ok(GisModel {
        doc_len: source.len() as u64,
        dialect,
        root: xml.root,
        records: if build { records_all } else { Vec::new() },
        points: if build { points_all } else { Vec::new() },
        xml,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `GisModel` node).
pub fn build_gis_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// The recognized root-level field node indices, in document order.
pub fn root_field_nodes(model: &GisModel, source: &[u8]) -> Result<Vec<u32>> {
    recognized_children(&model.xml, source, model.root, root_fields(model.dialect))
}

/// The recognized field node indices of the `index`-th record, in document order.
pub fn record_field_nodes(model: &GisModel, source: &[u8], index: u32) -> Result<Vec<u32>> {
    let node = *model
        .records
        .get(index as usize)
        .ok_or_else(|| Error::unsupported_feature(format!("geospatial has no record {index}")))?;
    recognized_children(&model.xml, source, node, record_fields(model.dialect))
}

/// The recognized field node indices of the `index`-th point, in document order.
pub fn point_field_nodes(model: &GisModel, source: &[u8], index: u32) -> Result<Vec<u32>> {
    let node = *model
        .points
        .get(index as usize)
        .ok_or_else(|| Error::unsupported_feature(format!("geospatial has no point {index}")))?;
    recognized_children(&model.xml, source, node, point_fields(model.dialect))
}

/// A field element's decoded text. Entity references are surfaced literally.
pub fn field_text(model: &GisModel, source: &[u8], node: u32) -> Result<String> {
    if model.node(node).is_none() {
        return Err(corrupt("geospatial field node is out of range"));
    }
    subtree_text(&model.xml, source, node)
}

/// The exact source bytes of a field element's whole span.
pub fn field_bytes<'a>(model: &GisModel, source: &'a [u8], node: u32) -> Result<&'a [u8]> {
    let n = model
        .xml
        .node(node)
        .ok_or_else(|| corrupt("geospatial field node is out of range"))?;
    token_bytes(source, n)
}

/// The local name of a field element (`prefix:` stripped).
pub fn field_name(model: &GisModel, source: &[u8], node: u32) -> Result<String> {
    let n = model
        .xml
        .node(node)
        .ok_or_else(|| corrupt("geospatial field node is out of range"))?;
    Ok(split_qname(element_name(source, n)?).1.to_string())
}

/// The attributes of a field element, in source order, with the qualified
/// spelling preserved.
pub fn field_attrs(model: &GisModel, source: &[u8], node: u32) -> Result<Vec<FieldAttr>> {
    let n = model
        .xml
        .node(node)
        .ok_or_else(|| corrupt("geospatial field node is out of range"))?;
    let mut out = Vec::new();
    for &a in &n.attrs {
        let attr = model
            .xml
            .attr(a)
            .ok_or_else(|| corrupt("geospatial attribute index is out of range"))?;
        out.push(FieldAttr {
            name: attr_name(source, attr)?.to_string(),
            value: String::from_utf8_lossy(attr_value_bytes(source, attr)?).into_owned(),
            start: attr.span_start,
            end: attr.span_end,
        });
    }
    Ok(out)
}

/// The value of the first attribute of `node` whose local name is `name`
/// (namespace-prefix stripped; namespace declarations are skipped).
pub fn attr_value_by_local(
    model: &GisModel,
    source: &[u8],
    node: u32,
    name: &str,
) -> Result<Option<String>> {
    let n = model
        .xml
        .node(node)
        .ok_or_else(|| corrupt("geospatial field node is out of range"))?;
    for &a in &n.attrs {
        let attr = model
            .xml
            .attr(a)
            .ok_or_else(|| corrupt("geospatial attribute index is out of range"))?;
        if attr.is_ns != 0 {
            continue;
        }
        if split_qname(attr_name(source, attr)?).1 == name {
            return Ok(Some(
                String::from_utf8_lossy(attr_value_bytes(source, attr)?).into_owned(),
            ));
        }
    }
    Ok(None)
}

/// A bounded, case-sensitive lexical search over recognized field values, in
/// document order (root fields first, then each record's fields, then each point's
/// fields).
pub fn find(model: &GisModel, source: &[u8], pattern: &str, max_out: u64) -> Result<Vec<GisMatch>> {
    let mut out: Vec<GisMatch> = Vec::new();
    for f in root_field_nodes(model, source)? {
        push_match(model, source, f, "root", None, pattern, max_out, &mut out)?;
    }
    for (i, &rec) in model.records.iter().enumerate() {
        for f in recognized_children(&model.xml, source, rec, record_fields(model.dialect))? {
            push_match(
                model,
                source,
                f,
                "record",
                Some(i as u32),
                pattern,
                max_out,
                &mut out,
            )?;
        }
    }
    for (i, &pt) in model.points.iter().enumerate() {
        for f in recognized_children(&model.xml, source, pt, point_fields(model.dialect))? {
            push_match(
                model,
                source,
                f,
                "point",
                Some(i as u32),
                pattern,
                max_out,
                &mut out,
            )?;
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn push_match(
    model: &GisModel,
    source: &[u8],
    node: u32,
    kind: &'static str,
    index: Option<u32>,
    pattern: &str,
    max_out: u64,
    out: &mut Vec<GisMatch>,
) -> Result<()> {
    let text = field_text(model, source, node)?;
    if !text.contains(pattern) {
        return Ok(());
    }
    let n = model
        .xml
        .node(node)
        .ok_or_else(|| corrupt("geospatial field node is out of range"))?;
    out.push(GisMatch {
        kind,
        index,
        name: split_qname(element_name(source, n)?).1.to_string(),
        start: n.start,
        end: n.end,
        text,
    });
    if out.len() as u64 > max_out {
        return Err(Error::resource_limit(format!(
            "geospatial find exceeded the {max_out}-match budget"
        )));
    }
    Ok(())
}

/// The whole document's canonical text: every recognized field value in document
/// order (root fields, then each record's fields, then each point's fields),
/// newline-separated.
pub fn canonical_text(model: &GisModel, source: &[u8], max_out: u64) -> Result<String> {
    let mut out = String::new();
    let mut groups: Vec<Vec<u32>> = Vec::new();
    groups.push(root_field_nodes(model, source)?);
    for i in 0..model.records.len() as u32 {
        groups.push(record_field_nodes(model, source, i)?);
    }
    for i in 0..model.points.len() as u32 {
        groups.push(point_field_nodes(model, source, i)?);
    }
    for group in groups {
        for f in group {
            out.push_str(&field_text(model, source, f)?);
            out.push('\n');
            if out.len() as u64 > max_out {
                return Err(Error::resource_limit(format!(
                    "geospatial text projection exceeds the {max_out}-byte budget"
                )));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Resolve the dialect, the record nodes, and the point nodes from a parsed XML
/// tree, enforcing the geospatial caps.
fn analyze(xml: &XmlModel, source: &[u8], limits: Limits) -> Result<(u8, Vec<u32>, Vec<u32>)> {
    let root_node = xml
        .node(xml.root)
        .ok_or_else(|| corrupt("model has no root node"))?;
    if root_node.kind != K_ELEMENT {
        return Err(corrupt("the geospatial root is not an element"));
    }
    let full = element_name(source, root_node)?;
    let (prefix, local) = split_qname(full);
    match local {
        "kml" => {
            if !ns_in_force(xml, source, xml.root, prefix, KML_NS)? {
                return Err(invalid_gis(
                    "`<kml>` is not in the KML namespace (http://www.opengis.net/kml/2.2)",
                ));
            }
            if !has_child_in(xml, source, xml.root, KML_CONTAINERS)? {
                return Err(invalid_gis(
                    "KML `<kml>` has no Document/Folder/Placemark child element",
                ));
            }
            let records = elements_by_local(xml, source, &["Placemark"])?;
            let points = elements_by_local(xml, source, KML_GEOMETRIES)?;
            check_caps(xml, source, DIALECT_KML, &records, &points, 0, limits)?;
            Ok((DIALECT_KML, records, points))
        }
        "gpx" => {
            if !ns_in_force(xml, source, xml.root, prefix, GPX_NS)? {
                return Err(invalid_gis(
                    "`<gpx>` is not in the GPX namespace (http://www.topografix.com/GPX/1/1)",
                ));
            }
            if !has_child_in(xml, source, xml.root, GPX_CHILDREN)? {
                return Err(invalid_gis(
                    "GPX `<gpx>` has no metadata/wpt/rte/trk child element",
                ));
            }
            let children = element_children(xml, xml.root)?;
            let mut records = Vec::new();
            let mut tracks = 0u64;
            for &c in &children {
                let x = &xml.nodes[c as usize];
                let n = split_qname(element_name(source, x)?).1;
                if GPX_RECORDS.contains(&n) {
                    records.push(c);
                }
                if n == "trk" {
                    tracks += 1;
                }
            }
            let points = elements_by_local(xml, source, GPX_POINTS)?;
            check_caps(xml, source, DIALECT_GPX, &records, &points, tracks, limits)?;
            Ok((DIALECT_GPX, records, points))
        }
        other => Err(invalid_gis(format!(
            "root element {other:?} is not a KML/GPX geospatial document"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
fn check_caps(
    xml: &XmlModel,
    source: &[u8],
    dialect: u8,
    records: &[u32],
    points: &[u32],
    tracks: u64,
    limits: Limits,
) -> Result<()> {
    if records.len() as u64 > u64::from(limits.max_gis_placemarks) {
        return Err(Error::resource_limit(format!(
            "geospatial document exceeds the {}-record cap",
            limits.max_gis_placemarks
        )));
    }
    if tracks > u64::from(limits.max_gis_tracks) {
        return Err(Error::resource_limit(format!(
            "geospatial document exceeds the {}-track cap",
            limits.max_gis_tracks
        )));
    }
    if points.len() as u64 > u64::from(limits.max_gis_points) {
        return Err(Error::resource_limit(format!(
            "geospatial document exceeds the {}-point cap",
            limits.max_gis_points
        )));
    }
    if u64::from(xml.max_depth) > u64::from(limits.max_gis_depth) {
        return Err(Error::resource_limit(format!(
            "geospatial document exceeds the {}-deep nesting cap",
            limits.max_gis_depth
        )));
    }
    // Aggregate the recognized-field bytes against the text cap.
    let mut bytes: u64 = 0;
    let mut charge = |nodes: Vec<u32>| -> Result<()> {
        for n in nodes {
            let node = xml
                .node(n)
                .ok_or_else(|| corrupt("geospatial field node is out of range"))?;
            bytes = bytes.saturating_add(node.end.saturating_sub(node.start));
            if bytes > limits.max_gis_text_bytes {
                return Err(Error::resource_limit(format!(
                    "geospatial document exceeds the {}-byte text cap",
                    limits.max_gis_text_bytes
                )));
            }
        }
        Ok(())
    };
    charge(recognized_children(
        xml,
        source,
        xml.root,
        root_fields(dialect),
    )?)?;
    for &r in records {
        charge(recognized_children(xml, source, r, record_fields(dialect))?)?;
    }
    for &p in points {
        charge(recognized_children(xml, source, p, point_fields(dialect))?)?;
    }
    Ok(())
}

/// Every element node in the whole tree whose local name is one of `names`, in
/// document order (the arena is pre-order).
fn elements_by_local(xml: &XmlModel, source: &[u8], names: &[&str]) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    for (i, node) in xml.nodes.iter().enumerate() {
        if node.kind != K_ELEMENT {
            continue;
        }
        if names.contains(&split_qname(element_name(source, node)?).1) {
            out.push(i as u32);
        }
    }
    Ok(out)
}

/// Whether `parent` has a direct child element whose local name is one of `names`.
fn has_child_in(xml: &XmlModel, source: &[u8], parent: u32, names: &[&str]) -> Result<bool> {
    for c in element_children(xml, parent)? {
        let x = &xml.nodes[c as usize];
        if names.contains(&split_qname(element_name(source, x)?).1) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The direct child element node indices of `parent`, in document order.
fn element_children(xml: &XmlModel, parent: u32) -> Result<Vec<u32>> {
    let node = xml
        .node(parent)
        .ok_or_else(|| corrupt("geospatial parent node is out of range"))?;
    let mut out = Vec::new();
    for &c in &node.children {
        let cn = xml
            .node(c)
            .ok_or_else(|| corrupt("geospatial child index is out of range"))?;
        if cn.kind == K_ELEMENT {
            out.push(c);
        }
    }
    Ok(out)
}

/// Every direct child element of `parent` whose local name is one of `names`, in
/// document order.
fn recognized_children(
    xml: &XmlModel,
    source: &[u8],
    parent: u32,
    names: &[&str],
) -> Result<Vec<u32>> {
    let node = xml
        .node(parent)
        .ok_or_else(|| corrupt("geospatial parent node is out of range"))?;
    let mut out = Vec::new();
    for &c in &node.children {
        let cn = xml
            .node(c)
            .ok_or_else(|| corrupt("geospatial child index is out of range"))?;
        if cn.kind != K_ELEMENT {
            continue;
        }
        if names.contains(&split_qname(element_name(source, cn)?).1) {
            out.push(c);
        }
    }
    Ok(out)
}

/// Whether the element `node` puts the namespace `prefix` (empty for the default
/// namespace) in force, bound to `uri`. Only the element's own declarations are
/// consulted — the root is the document element, so there is no enclosing scope.
fn ns_in_force(xml: &XmlModel, source: &[u8], node: u32, prefix: &str, uri: &str) -> Result<bool> {
    let n = xml
        .node(node)
        .ok_or_else(|| corrupt("geospatial root node is out of range"))?;
    for &a in &n.attrs {
        let attr = xml
            .attr(a)
            .ok_or_else(|| corrupt("geospatial attribute index is out of range"))?;
        if attr.is_ns == 0 {
            continue;
        }
        let name = attr_name(source, attr)?;
        let matches = if prefix.is_empty() {
            name == "xmlns"
        } else {
            name.strip_prefix("xmlns:") == Some(prefix)
        };
        if matches {
            return Ok(attr_value_bytes(source, attr)? == uri.as_bytes());
        }
    }
    Ok(false)
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_gis_structure(msg)
}

fn invalid_gis(msg: impl Into<String>) -> Error {
    Error::invalid_gis_structure(msg)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("model offset overflow"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("model is truncated"))?;
        self.at = end;
        Ok(s)
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

    fn at_end(&self) -> bool {
        self.at >= self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KML: &[u8] = b"<?xml version=\"1.0\"?>\n\
<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<Document>\n<name>Example</name>\n\
<Placemark>\n<name>First</name>\n<description>one</description>\n\
<styleUrl>#s1</styleUrl>\n<Point><coordinates>1.25,-2.5e1,0</coordinates></Point>\n\
</Placemark>\n<Placemark>\n<name>Second</name>\n\
<LineString><coordinates>0,0 1,1</coordinates></LineString>\n</Placemark>\n\
</Document>\n</kml>\n";

    const GPX: &[u8] = b"<?xml version=\"1.0\"?>\n\
<gpx xmlns=\"http://www.topografix.com/GPX/1/1\" version=\"1.1\" creator=\"test\">\n\
<metadata><name>Example</name></metadata>\n\
<wpt lat=\"1.0\" lon=\"2.0\"><ele>10</ele><time>2024-01-01T00:00:00Z</time><name>W</name></wpt>\n\
<rte><name>R</name><rtept lat=\"3.0\" lon=\"4.0\"><ele>20</ele></rtept></rte>\n\
<trk><name>T</name><trkseg><trkpt lat=\"5.0\" lon=\"6.0\"><ele>30</ele>\
<time>2024-01-02T00:00:00Z</time></trkpt></trkseg></trk>\n</gpx>\n";

    fn model(src: &[u8]) -> GisModel {
        parse(src, Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_kml_and_gpx_and_rejects_junk() {
        assert_eq!(classify(KML, Limits::DEFAULT).unwrap(), DIALECT_KML);
        assert_eq!(classify(GPX, Limits::DEFAULT).unwrap(), DIALECT_GPX);
        // A plain XML document is not geospatial.
        assert!(classify(b"<a><b>text</b></a>", Limits::DEFAULT).is_err());
        // `<kml>` with no KML namespace declines.
        assert!(classify(b"<kml><Document/></kml>", Limits::DEFAULT).is_err());
        // `<kml>` in the KML namespace but with no structural child declines.
        assert!(
            classify(
                b"<kml xmlns=\"http://www.opengis.net/kml/2.2\"/>",
                Limits::DEFAULT
            )
            .is_err()
        );
        // `<gpx>` with no GPX namespace declines.
        assert!(classify(b"<gpx><wpt/></gpx>", Limits::DEFAULT).is_err());
        // `<gpx>` in the GPX namespace but with no structural child declines.
        assert!(
            classify(
                b"<gpx xmlns=\"http://www.topografix.com/GPX/1/1\"/>",
                Limits::DEFAULT
            )
            .is_err()
        );
        // Prose and junk decline.
        assert!(classify(b"not xml at all", Limits::DEFAULT).is_err());
    }

    #[test]
    fn kml_preserves_spans_order_and_geometry() {
        let m = model(KML);
        assert_eq!(m.dialect, DIALECT_KML);
        assert_eq!(m.record_count(), 2);
        assert_eq!(m.point_count(), 2);
        assert!(m.records[0] < m.records[1]);
        // The first placemark's name span is the exact source bytes.
        let fields = record_field_nodes(&m, KML, 0).unwrap();
        let name = fields[0];
        assert_eq!(field_name(&m, KML, name).unwrap(), "name");
        assert_eq!(field_bytes(&m, KML, name).unwrap(), b"<name>First</name>");
        assert_eq!(field_text(&m, KML, name).unwrap(), "First");
        // The geometry keeps its coordinates text.
        let point = m.points[0];
        let coords = point_field_nodes(&m, KML, 0).unwrap();
        assert_eq!(field_text(&m, KML, coords[0]).unwrap(), "1.25,-2.5e1,0");
        assert_eq!(field_name(&m, KML, point).unwrap(), "Point");
        // The namespace declaration is preserved as an attribute on the root.
        let root = m.node(m.root).unwrap();
        let ns = m.xml.attr(root.attrs[0]).unwrap();
        assert_eq!(ns.is_ns, 1);
        assert_eq!(attr_name(KML, ns).unwrap(), "xmlns");
        assert_eq!(attr_value_bytes(KML, ns).unwrap(), KML_NS.as_bytes());
    }

    #[test]
    fn gpx_preserves_lat_lon_attributes_and_ele_time() {
        let m = model(GPX);
        assert_eq!(m.dialect, DIALECT_GPX);
        // Records: one wpt, one rte, one trk.
        assert_eq!(m.record_count(), 3);
        // Points: wpt + rtept + trkpt = 3, in document order.
        assert_eq!(m.point_count(), 3);
        assert!(m.points[0] < m.points[1] && m.points[1] < m.points[2]);
        // The trkpt `lat`/`lon` spellings survive as attributes.
        let trkpt = m.points[2];
        assert_eq!(
            attr_value_by_local(&m, GPX, trkpt, "lat").unwrap().unwrap(),
            "5.0"
        );
        assert_eq!(
            attr_value_by_local(&m, GPX, trkpt, "lon").unwrap().unwrap(),
            "6.0"
        );
        let attrs = field_attrs(&m, GPX, trkpt).unwrap();
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].name, "lat");
        assert_eq!(attrs[1].name, "lon");
        // The trkpt `ele`/`time` are child elements with exact text.
        let fields = point_field_nodes(&m, GPX, 2).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(field_text(&m, GPX, fields[0]).unwrap(), "30");
        assert_eq!(
            field_text(&m, GPX, fields[1]).unwrap(),
            "2024-01-02T00:00:00Z"
        );
        // The namespace declaration is preserved.
        let root = m.node(m.root).unwrap();
        let ns = m.xml.attr(root.attrs[0]).unwrap();
        assert_eq!(ns.is_ns, 1);
        assert_eq!(attr_value_bytes(GPX, ns).unwrap(), GPX_NS.as_bytes());
    }

    #[test]
    fn model_roundtrips() {
        for src in [KML, GPX] {
            let m = model(src);
            let enc = m.encode();
            assert_eq!(GisModel::decode(&enc).unwrap(), m);
            let mut bad = enc.clone();
            bad[0] = b'X';
            assert!(GisModel::decode(&bad).is_err());
            let mut truncated = enc;
            truncated.truncate(truncated.len() - 1);
            assert!(GisModel::decode(&truncated).is_err());
        }
    }

    #[test]
    fn canonical_text_and_find() {
        let m = model(KML);
        let text = canonical_text(&m, KML, 1 << 20).unwrap();
        assert!(text.contains("First"));
        assert!(text.contains("1.25,-2.5e1,0"));
        let hits = find(&m, KML, "First", 1 << 20).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "record");
        assert_eq!(hits[0].index, Some(0));
        assert_eq!(hits[0].name, "name");
    }

    #[test]
    fn caps_decline_typed() {
        // A KML over the STRICT point cap (1 << 14).
        let mut src = String::from("<kml xmlns=\"http://www.opengis.net/kml/2.2\"><Document>");
        for _ in 0..20000 {
            src.push_str("<Placemark><Point><coordinates>0,0</coordinates></Point></Placemark>");
        }
        src.push_str("</Document></kml>");
        let e = parse(src.as_bytes(), Limits::STRICT, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x6151_2020_23DE_AD00;
        for _ in 0..128 {
            let mut buf = vec![0u8; 512];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = canonical_text(&m, &buf, 1 << 20);
                let _ = build_gis_model(&buf, Limits::STRICT);
            }
        }
    }
}
