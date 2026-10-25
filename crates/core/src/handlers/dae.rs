//! COLLADA (`.dae`) handler.
//!
//! COLLADA is an open, XML-based scene exchange format with two halves that
//! matter here:
//!
//! * `<library_geometries>` — reusable `<mesh>`es made of `<source>` float
//!   arrays (positions), a `<vertices>` indirection, and triangle-bearing
//!   primitives (`<triangles>`, `<polylist>`, `<polygons>`, `<tristrips>`,
//!   `<trifans>`).
//! * `<library_visual_scenes>` — a node hierarchy that instantiates geometries
//!   with `<matrix>`/`<translate>`/`<rotate>`/`<scale>` transforms, so an
//!   assembly renders in place.
//!
//! The geometry is already triangles, so like STL/OBJ/PLY/3MF this maps straight
//! onto a [`Mesh`] with no external converter or WebAssembly kernel. Primitives
//! are fan-triangulated, node hierarchies are composed, and indices that fall
//! outside the position array are dropped rather than trusted.

use std::collections::BTreeMap;

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Reader;

use crate::{Error, Mesh, Result};

use super::{basic_attributes, fmt_vec3, CadHandler, FileMetadata};

/// Row-major 4×4 identity. COLLADA stores matrices row-major; points are
/// transformed as column vectors, so translation lives in column 3
/// (flat indices 3, 7, 11).
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// COLLADA handler: `.dae`.
pub struct DaeHandler;

impl CadHandler for DaeHandler {
    fn name(&self) -> &'static str {
        "dae"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["dae"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let doc = parse(bytes)?;
        let asset = &doc.asset;
        let mut attributes = basic_attributes(path, bytes);
        insert(&mut attributes, "version", &asset.version);
        insert(&mut attributes, "author", &asset.author);
        insert(&mut attributes, "authoring_tool", &asset.authoring_tool);
        insert(&mut attributes, "created", &asset.created);
        insert(&mut attributes, "modified", &asset.modified);
        insert(&mut attributes, "unit_name", &asset.unit_name);
        if let Some(meter) = asset.unit_meter {
            attributes.insert("unit_meter".into(), meter.to_string());
        }
        insert(&mut attributes, "up_axis", &asset.up_axis);
        attributes.insert("geometries".into(), doc.geometry_count.to_string());
        attributes.insert("nodes".into(), doc.node_count.to_string());
        attributes.insert("vertices".into(), doc.mesh.vertex_count().to_string());
        attributes.insert("triangles".into(), doc.mesh.triangle_count().to_string());
        if let Some((min, max)) = doc.mesh.bounds() {
            attributes.insert("bounds_min".into(), fmt_vec3(min));
            attributes.insert("bounds_max".into(), fmt_vec3(max));
        }
        attributes.insert("volume".into(), format!("{:.4}", doc.mesh.volume()));
        Ok(FileMetadata {
            format: "dae".into(),
            attributes,
        })
    }

    fn mesh(&self, _path: &str, bytes: &[u8]) -> Result<Option<Mesh>> {
        Ok(Some(parse(bytes)?.mesh))
    }
}

fn insert(attributes: &mut BTreeMap<String, String>, key: &str, value: &str) {
    if !value.is_empty() {
        attributes.insert(key.to_string(), value.to_string());
    }
}

/// `<asset>` metadata.
#[derive(Default)]
struct Asset {
    version: String,
    author: String,
    authoring_tool: String,
    created: String,
    modified: String,
    unit_name: String,
    unit_meter: Option<f32>,
    up_axis: String,
}

/// A `<source>` float array (positions).
#[derive(Default)]
struct Source {
    values: Vec<f32>,
}

/// A resolved `<geometry><mesh>`: positions plus triangle indices.
#[derive(Default)]
struct Geometry {
    id: String,
    positions: Vec<f32>,
    indices: Vec<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum PrimKind {
    #[default]
    Unknown,
    Triangles,
    Polylist,
    Polygons,
    TriStrips,
    TriFans,
}

/// One `<input>` of a primitive.
#[derive(Clone)]
struct Input {
    semantic: String,
    offset: usize,
    source: String,
}

/// A primitive (`<triangles>` …) being read.
#[derive(Default)]
struct Primitive {
    kind: PrimKind,
    inputs: Vec<Input>,
    vcounts: Vec<usize>,
}

/// A node in the visual-scene hierarchy.
#[derive(Default)]
struct Node {
    local: Option<[f32; 16]>,
    instances: Vec<String>,
    children: Vec<Node>,
}

/// The parsed document: summary counts plus the assembled mesh.
struct ColladaDocument {
    asset: Asset,
    geometry_count: usize,
    node_count: usize,
    mesh: Mesh,
}

/// Text currently being accumulated between an element's start and end.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Capture {
    #[default]
    None,
    FloatArray,
    VCount,
    P,
    Matrix,
    Translate,
    Rotate,
    Scale,
    Author,
    AuthoringTool,
    Created,
    Modified,
    UpAxis,
}

/// A single-pass streaming parser for a COLLADA document.
#[derive(Default)]
struct Parser {
    asset: Asset,
    sources: BTreeMap<String, Source>,
    vertices: BTreeMap<String, String>,
    geometries: Vec<Geometry>,
    current_source: Option<String>,
    current_vertices: Option<String>,
    current_geometry: Option<usize>,
    in_vertices: bool,
    in_library_nodes: bool,
    current_prim: Option<Primitive>,
    stack: Vec<Node>,
    roots: Vec<Node>,
    capture: Capture,
    buffer: String,
}

impl Parser {
    fn on_start(&mut self, e: &BytesStart<'_>) {
        match e.local_name().as_ref() {
            b"COLLADA" => {
                if let Some(version) = attr(e, b"version") {
                    self.asset.version = version;
                }
            }
            b"source" => {
                let id = attr(e, b"id").unwrap_or_default();
                self.sources.entry(id.clone()).or_default();
                self.current_source = Some(id);
            }
            b"geometry" => {
                let id = attr(e, b"id").unwrap_or_default();
                self.geometries.push(Geometry {
                    id,
                    ..Default::default()
                });
                self.current_geometry = Some(self.geometries.len() - 1);
            }
            b"vertices" => {
                let id = attr(e, b"id").unwrap_or_default();
                self.current_vertices = Some(id);
                self.in_vertices = true;
            }
            b"float_array" => self.begin_capture(Capture::FloatArray),
            b"vcount" => self.begin_capture(Capture::VCount),
            b"p" => self.begin_capture(Capture::P),
            b"matrix" => self.begin_capture(Capture::Matrix),
            b"translate" => self.begin_capture(Capture::Translate),
            b"rotate" => self.begin_capture(Capture::Rotate),
            b"scale" => self.begin_capture(Capture::Scale),
            b"author" => self.begin_capture(Capture::Author),
            b"authoring_tool" => self.begin_capture(Capture::AuthoringTool),
            b"created" => self.begin_capture(Capture::Created),
            b"modified" => self.begin_capture(Capture::Modified),
            b"up_axis" => self.begin_capture(Capture::UpAxis),
            b"unit" => self.read_unit(e),
            b"input" => self.read_input(e),
            b"triangles" => self.begin_primitive(PrimKind::Triangles),
            b"polylist" => self.begin_primitive(PrimKind::Polylist),
            b"polygons" => self.begin_primitive(PrimKind::Polygons),
            b"tristrips" => self.begin_primitive(PrimKind::TriStrips),
            b"trifans" => self.begin_primitive(PrimKind::TriFans),
            b"library_nodes" => self.in_library_nodes = true,
            b"node" if !self.in_library_nodes => self.stack.push(Node::default()),
            _ => {}
        }
    }

    fn on_empty(&mut self, e: &BytesStart<'_>) {
        match e.local_name().as_ref() {
            b"unit" => self.read_unit(e),
            b"input" => self.read_input(e),
            b"instance_geometry" | b"instance_node" => {
                if let Some(url) = attr(e, b"url") {
                    if let Some(node) = self.stack.last_mut() {
                        node.instances.push(strip_hash(&url).to_string());
                    }
                }
            }
            b"node" if !self.in_library_nodes => {
                let node = Node::default();
                attach_node(&mut self.stack, &mut self.roots, node);
            }
            _ => {}
        }
    }

    fn on_text(&mut self, text: &BytesText<'_>) {
        if self.capture != Capture::None {
            if let Ok(value) = text.unescape() {
                self.buffer.push_str(&value);
            }
        }
    }

    fn on_end(&mut self, e: &BytesEnd<'_>) {
        match e.local_name().as_ref() {
            b"float_array" => {
                let values = parse_floats(&self.buffer);
                if let Some(id) = self.current_source.clone() {
                    if let Some(source) = self.sources.get_mut(&id) {
                        source.values = values;
                    }
                }
                self.end_capture();
            }
            b"vcount" => {
                let counts: Vec<usize> = parse_ints(&self.buffer)
                    .into_iter()
                    .map(|value| value.max(0) as usize)
                    .collect();
                if let Some(prim) = self.current_prim.as_mut() {
                    prim.vcounts = counts;
                }
                self.end_capture();
            }
            b"p" => {
                let values = parse_ints(&self.buffer);
                self.flush_primitive(&values);
                self.end_capture();
            }
            b"matrix" | b"translate" | b"rotate" | b"scale" => {
                let kind = match e.local_name().as_ref() {
                    b"matrix" => Capture::Matrix,
                    b"translate" => Capture::Translate,
                    b"rotate" => Capture::Rotate,
                    _ => Capture::Scale,
                };
                if let Some(matrix) = transform_from(kind, &self.buffer) {
                    if let Some(node) = self.stack.last_mut() {
                        node.local = Some(match node.local {
                            Some(existing) => mul(existing, matrix),
                            None => matrix,
                        });
                    }
                }
                self.end_capture();
            }
            b"author" => {
                let value = self.take_field();
                self.asset.author = value;
            }
            b"authoring_tool" => {
                let value = self.take_field();
                self.asset.authoring_tool = value;
            }
            b"created" => {
                let value = self.take_field();
                self.asset.created = value;
            }
            b"modified" => {
                let value = self.take_field();
                self.asset.modified = value;
            }
            b"up_axis" => {
                let value = self.take_field();
                self.asset.up_axis = value;
            }
            b"source" => self.current_source = None,
            b"vertices" => {
                self.in_vertices = false;
                self.current_vertices = None;
            }
            b"geometry" => self.current_geometry = None,
            b"triangles" | b"polylist" | b"polygons" | b"tristrips" | b"trifans" => {
                self.current_prim = None;
            }
            b"library_nodes" => self.in_library_nodes = false,
            b"node" if !self.in_library_nodes => {
                if let Some(node) = self.stack.pop() {
                    attach_node(&mut self.stack, &mut self.roots, node);
                }
            }
            _ => {}
        }
    }

    fn begin_capture(&mut self, kind: Capture) {
        self.capture = kind;
        self.buffer.clear();
    }

    fn end_capture(&mut self) {
        self.capture = Capture::None;
        self.buffer.clear();
    }

    /// Finish a text field, returning its trimmed contents and resetting state.
    fn take_field(&mut self) -> String {
        let value = self.buffer.trim().to_string();
        self.end_capture();
        value
    }

    fn begin_primitive(&mut self, kind: PrimKind) {
        self.current_prim = Some(Primitive {
            kind,
            ..Default::default()
        });
    }

    fn read_unit(&mut self, e: &BytesStart<'_>) {
        if let Some(name) = attr(e, b"name") {
            self.asset.unit_name = name;
        }
        if let Some(meter) = attr(e, b"meter").and_then(|m| m.parse::<f32>().ok()) {
            self.asset.unit_meter = Some(meter);
        }
    }

    fn read_input(&mut self, e: &BytesStart<'_>) {
        let semantic = attr(e, b"semantic").unwrap_or_default();
        let source = attr(e, b"source").unwrap_or_default();
        if self.in_vertices {
            if semantic == "POSITION" {
                if let Some(id) = self.current_vertices.clone() {
                    self.vertices.insert(id, strip_hash(&source).to_string());
                }
            }
        } else if let Some(prim) = self.current_prim.as_mut() {
            let offset = attr(e, b"offset")
                .and_then(|o| o.parse::<usize>().ok())
                .unwrap_or(0);
            prim.inputs.push(Input {
                semantic,
                offset,
                source,
            });
        }
    }

    /// Triangulate the `<p>` payload of the active primitive into the geometry.
    fn flush_primitive(&mut self, values: &[i64]) {
        let Some(index) = self.current_geometry else {
            return;
        };
        let Some(prim) = self.current_prim.as_ref() else {
            return;
        };
        let kind = prim.kind;
        let vcounts = prim.vcounts.clone();
        let stride = prim
            .inputs
            .iter()
            .map(|input| input.offset)
            .max()
            .map_or(1, |max| max + 1);
        let Some(vertex) = prim.inputs.iter().find(|input| input.semantic == "VERTEX") else {
            return;
        };
        let vertex_offset = vertex.offset;
        let vertex_source = strip_hash(&vertex.source).to_string();

        // VERTEX points at a `<vertices>` element, which maps POSITION to the
        // `<source>` that actually holds the coordinates.
        let Some(position_source) = self.vertices.get(&vertex_source) else {
            return;
        };
        let Some(source) = self.sources.get(position_source) else {
            return;
        };
        let positions = source.values.clone();

        let geometry = &mut self.geometries[index];
        if geometry.positions.is_empty() {
            geometry.positions = positions;
        }
        let vertex_count = geometry.positions.len() / 3;

        for triangle in triangulate(kind, values, &vcounts, stride, vertex_offset) {
            if triangle.iter().all(|&i| (i as usize) < vertex_count) {
                geometry.indices.extend_from_slice(&triangle);
            }
        }
    }

    fn finish(self) -> ColladaDocument {
        let by_id: BTreeMap<&str, usize> = self
            .geometries
            .iter()
            .enumerate()
            .map(|(index, geometry)| (geometry.id.as_str(), index))
            .collect();
        let mesh = assemble(&self.geometries, &by_id, &self.roots);
        let node_count = self.roots.iter().map(count_nodes).sum();
        ColladaDocument {
            asset: self.asset,
            geometry_count: self.geometries.len(),
            node_count,
            mesh,
        }
    }
}

/// Parse a `.dae` file into a document summary and a triangle mesh.
fn parse(bytes: &[u8]) -> Result<ColladaDocument> {
    let text = String::from_utf8_lossy(bytes);
    if !text.contains("COLLADA") {
        return Err(Error::parse("dae", "missing `<COLLADA>` root element"));
    }
    let mut reader = Reader::from_str(&text);
    let mut parser = Parser::default();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(Error::parse("dae", format!("malformed XML: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => parser.on_start(&e),
            Ok(Event::Empty(e)) => parser.on_empty(&e),
            Ok(Event::Text(t)) => parser.on_text(&t),
            Ok(Event::End(e)) => parser.on_end(&e),
            _ => {}
        }
        buf.clear();
    }

    Ok(parser.finish())
}

/// Convert a primitive's `<p>` payload into triangles.
fn triangulate(
    kind: PrimKind,
    values: &[i64],
    vcounts: &[usize],
    stride: usize,
    vertex_offset: usize,
) -> Vec<[u32; 3]> {
    let vertex_at = |slot: usize| -> Option<u32> {
        let position = slot * stride + vertex_offset;
        values.get(position).map(|value| (*value).max(0) as u32)
    };
    let strip: Vec<u32> = (0..values.len() / stride).filter_map(vertex_at).collect();

    let mut triangles = Vec::new();
    match kind {
        PrimKind::Triangles => {
            for group in 0..values.len() / (stride * 3) {
                if let (Some(a), Some(b), Some(c)) = (
                    vertex_at(group * 3),
                    vertex_at(group * 3 + 1),
                    vertex_at(group * 3 + 2),
                ) {
                    triangles.push([a, b, c]);
                }
            }
        }
        PrimKind::Polygons | PrimKind::Polylist => {
            let groups: Vec<usize> = if kind == PrimKind::Polylist {
                vcounts.to_vec()
            } else {
                vec![values.len() / stride]
            };
            let mut slot = 0;
            for count in groups {
                let polygon: Vec<u32> = (0..count)
                    .filter_map(|_| {
                        let value = vertex_at(slot);
                        slot += 1;
                        value
                    })
                    .collect();
                for k in 1..polygon.len().saturating_sub(1) {
                    triangles.push([polygon[0], polygon[k], polygon[k + 1]]);
                }
            }
        }
        PrimKind::TriStrips => {
            for i in 0..strip.len().saturating_sub(2) {
                if i % 2 == 0 {
                    triangles.push([strip[i], strip[i + 1], strip[i + 2]]);
                } else {
                    triangles.push([strip[i + 1], strip[i], strip[i + 2]]);
                }
            }
        }
        PrimKind::TriFans => {
            for i in 1..strip.len().saturating_sub(1) {
                triangles.push([strip[0], strip[i], strip[i + 1]]);
            }
        }
        PrimKind::Unknown => {}
    }
    triangles
}

/// Resolve the visual-scene hierarchy (or, absent instances, every geometry).
fn assemble(geometries: &[Geometry], by_id: &BTreeMap<&str, usize>, roots: &[Node]) -> Mesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();

    if roots.is_empty() || !roots.iter().any(has_instance) {
        for geometry in geometries {
            append_geometry(&mut positions, &mut indices, geometry, IDENTITY);
        }
    } else {
        for node in roots {
            walk(node, IDENTITY, geometries, by_id, &mut positions, &mut indices);
        }
    }

    Mesh::new(positions, indices)
}

fn walk(
    node: &Node,
    parent: [f32; 16],
    geometries: &[Geometry],
    by_id: &BTreeMap<&str, usize>,
    positions: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let world = mul(parent, node.local.unwrap_or(IDENTITY));
    for instance in &node.instances {
        if let Some(&index) = by_id.get(instance.as_str()) {
            append_geometry(positions, indices, &geometries[index], world);
        }
    }
    for child in &node.children {
        walk(child, world, geometries, by_id, positions, indices);
    }
}

fn append_geometry(
    positions: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    geometry: &Geometry,
    transform: [f32; 16],
) {
    let base = (positions.len() / 3) as u32;
    for vertex in geometry.positions.chunks_exact(3) {
        positions.extend_from_slice(&apply(&transform, vertex));
    }
    let vertex_count = (geometry.positions.len() / 3) as u32;
    for triangle in geometry.indices.chunks_exact(3) {
        if triangle.iter().any(|&index| index >= vertex_count) {
            continue;
        }
        indices.push(base + triangle[0]);
        indices.push(base + triangle[1]);
        indices.push(base + triangle[2]);
    }
}

fn has_instance(node: &Node) -> bool {
    !node.instances.is_empty() || node.children.iter().any(has_instance)
}

fn count_nodes(node: &Node) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

fn attach_node(stack: &mut Vec<Node>, roots: &mut Vec<Node>, node: Node) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => roots.push(node),
    }
}

/// Build a row-major matrix from a transform element's text payload.
fn transform_from(kind: Capture, text: &str) -> Option<[f32; 16]> {
    let values = parse_floats(text);
    match kind {
        Capture::Matrix => {
            if values.len() != 16 {
                return None;
            }
            let mut matrix = [0f32; 16];
            matrix.copy_from_slice(&values);
            Some(matrix)
        }
        Capture::Translate => {
            let mut matrix = IDENTITY;
            matrix[3] = values.first().copied().unwrap_or(0.0);
            matrix[7] = values.get(1).copied().unwrap_or(0.0);
            matrix[11] = values.get(2).copied().unwrap_or(0.0);
            Some(matrix)
        }
        Capture::Scale => {
            let mut matrix = IDENTITY;
            matrix[0] = values.first().copied().unwrap_or(1.0);
            matrix[5] = values.get(1).copied().unwrap_or(1.0);
            matrix[10] = values.get(2).copied().unwrap_or(1.0);
            Some(matrix)
        }
        Capture::Rotate => {
            let axis = [
                values.first().copied().unwrap_or(0.0),
                values.get(1).copied().unwrap_or(0.0),
                values.get(2).copied().unwrap_or(0.0),
            ];
            let angle = values.get(3).copied().unwrap_or(0.0).to_radians();
            Some(rotation(axis, angle))
        }
        _ => None,
    }
}

/// Row-major rotation matrix (column-vector convention) for an axis-angle.
fn rotation(axis: [f32; 3], angle: f32) -> [f32; 16] {
    let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if length == 0.0 {
        return IDENTITY;
    }
    let (x, y, z) = (axis[0] / length, axis[1] / length, axis[2] / length);
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    [
        t * x * x + c, t * x * y - s * z, t * x * z + s * y, 0.0, //
        t * x * y + s * z, t * y * y + c, t * y * z - s * x, 0.0, //
        t * x * z - s * y, t * y * z + s * x, t * z * z + c, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

/// Row-major matrix product (column-vector convention): applies `b`, then `a`.
fn mul(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0f32; 16];
    for row in 0..4 {
        for col in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[row * 4 + k] * b[k * 4 + col];
            }
            out[row * 4 + col] = sum;
        }
    }
    out
}

/// Transform a point by a row-major matrix (column-vector convention).
fn apply(m: &[f32; 16], p: &[f32]) -> [f32; 3] {
    [
        m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
        m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
        m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
    ]
}

fn parse_floats(text: &str) -> Vec<f32> {
    text.split_whitespace()
        .filter_map(|token| token.parse::<f32>().ok())
        .collect()
}

fn parse_ints(text: &str) -> Vec<i64> {
    text.split_whitespace()
        .filter_map(|token| token.parse::<i64>().ok())
        .collect()
}

fn strip_hash(value: &str) -> &str {
    value.strip_prefix('#').unwrap_or(value)
}

/// Read an XML attribute by local name (namespace-prefix agnostic).
fn attr(element: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == key)
        .and_then(|attribute| attribute.unescape_value().ok().map(|value| value.into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRIANGLE: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<COLLADA xmlns="http://www.collada.org/2005/11/COLLADASchema" version="1.4.1">
  <asset>
    <contributor><author>Ada</author><authoring_tool>OpenPDM Test</authoring_tool></contributor>
    <created>2026-01-02T03:04:05</created>
    <unit meter="0.001" name="millimeter"/>
    <up_axis>Z_UP</up_axis>
  </asset>
  <library_geometries>
    <geometry id="tri" name="tri">
      <mesh>
        <source id="tri-positions">
          <float_array id="tri-positions-array" count="9">0 0 0 1 0 0 0 1 0</float_array>
          <technique_common>
            <accessor source="#tri-positions-array" count="3" stride="3"/>
          </technique_common>
        </source>
        <vertices id="tri-vertices">
          <input semantic="POSITION" source="#tri-positions"/>
        </vertices>
        <triangles count="1">
          <input semantic="VERTEX" source="#tri-vertices" offset="0"/>
          <p>0 1 2</p>
        </triangles>
      </mesh>
    </geometry>
  </library_geometries>
  <library_visual_scenes>
    <visual_scene id="scene">
      <node id="root"><instance_geometry url="#tri"/></node>
    </visual_scene>
  </library_visual_scenes>
</COLLADA>"##;

    #[test]
    fn parses_a_simple_triangle() {
        let mesh = DaeHandler.mesh("t.dae", TRIANGLE.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.bounds(), Some(([0.0, 0.0, 0.0], [1.0, 1.0, 0.0])));
    }

    #[test]
    fn extracts_asset_metadata() {
        let meta = DaeHandler.metadata("t.dae", TRIANGLE.as_bytes()).unwrap();
        assert_eq!(meta.format, "dae");
        assert_eq!(meta.attributes.get("version").map(String::as_str), Some("1.4.1"));
        assert_eq!(meta.attributes.get("author").map(String::as_str), Some("Ada"));
        assert_eq!(
            meta.attributes.get("authoring_tool").map(String::as_str),
            Some("OpenPDM Test")
        );
        assert_eq!(
            meta.attributes.get("unit_name").map(String::as_str),
            Some("millimeter")
        );
        assert_eq!(meta.attributes.get("unit_meter").map(String::as_str), Some("0.001"));
        assert_eq!(meta.attributes.get("up_axis").map(String::as_str), Some("Z_UP"));
        assert_eq!(meta.attributes.get("geometries").map(String::as_str), Some("1"));
        assert_eq!(meta.attributes.get("nodes").map(String::as_str), Some("1"));
    }

    #[test]
    fn parses_polylist_with_vcounts() {
        let dae = r##"<COLLADA version="1.4.1"><library_geometries>
  <geometry id="q"><mesh>
    <source id="q-pos">
      <float_array id="q-pos-array" count="12">0 0 0 1 0 0 1 1 0 0 1 0</float_array>
    </source>
    <vertices id="q-verts"><input semantic="POSITION" source="#q-pos"/></vertices>
    <polylist count="1">
      <input semantic="VERTEX" source="#q-verts" offset="0"/>
      <vcount>4</vcount>
      <p>0 1 2 3</p>
    </polylist>
  </mesh></geometry>
</library_geometries></COLLADA>"##;
        let mesh = DaeHandler.mesh("q.dae", dae.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 2);
    }

    #[test]
    fn parses_polygons_with_multiple_p_elements() {
        let dae = r##"<COLLADA version="1.4.1"><library_geometries>
  <geometry id="p"><mesh>
    <source id="p-pos">
      <float_array id="p-pos-array" count="12">0 0 0 1 0 0 1 1 0 0 1 0</float_array>
    </source>
    <vertices id="p-verts"><input semantic="POSITION" source="#p-pos"/></vertices>
    <polygons count="2">
      <input semantic="VERTEX" source="#p-verts" offset="0"/>
      <p>0 1 2</p>
      <p>0 2 3</p>
    </polygons>
  </mesh></geometry>
</library_geometries></COLLADA>"##;
        let mesh = DaeHandler.mesh("p.dae", dae.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 2);
    }

    #[test]
    fn handles_interleaved_input_offsets() {
        // VERTEX offset 0, NORMAL offset 1 -> stride 2.
        let dae = r##"<COLLADA version="1.4.1"><library_geometries>
  <geometry id="n"><mesh>
    <source id="n-pos">
      <float_array id="n-pos-array" count="9">0 0 0 1 0 0 0 1 0</float_array>
    </source>
    <vertices id="n-verts"><input semantic="POSITION" source="#n-pos"/></vertices>
    <triangles count="1">
      <input semantic="VERTEX" source="#n-verts" offset="0"/>
      <input semantic="NORMAL" source="#n-normals" offset="1"/>
      <p>0 0 1 1 2 2</p>
    </triangles>
  </mesh></geometry>
</library_geometries></COLLADA>"##;
        let mesh = DaeHandler.mesh("n.dae", dae.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn applies_node_matrix_translation() {
        let dae = r##"<COLLADA version="1.4.1">
  <library_geometries>
    <geometry id="g"><mesh>
      <source id="g-pos">
        <float_array id="g-pos-array" count="9">0 0 0 1 0 0 0 1 0</float_array>
      </source>
      <vertices id="g-verts"><input semantic="POSITION" source="#g-pos"/></vertices>
      <triangles count="1"><input semantic="VERTEX" source="#g-verts" offset="0"/><p>0 1 2</p></triangles>
    </mesh></geometry>
  </library_geometries>
  <library_visual_scenes><visual_scene id="s">
    <node id="n">
      <matrix>1 0 0 10 0 1 0 20 0 0 1 30 0 0 0 1</matrix>
      <instance_geometry url="#g"/>
    </node>
  </visual_scene></library_visual_scenes>
</COLLADA>"##;
        let mesh = DaeHandler.mesh("m.dae", dae.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.bounds(), Some(([10.0, 20.0, 30.0], [11.0, 21.0, 30.0])));
    }

    #[test]
    fn composes_nested_node_transforms() {
        let dae = r##"<COLLADA version="1.4.1">
  <library_geometries>
    <geometry id="g"><mesh>
      <source id="g-pos"><float_array id="g-pos-array" count="9">0 0 0 1 0 0 0 1 0</float_array></source>
      <vertices id="g-verts"><input semantic="POSITION" source="#g-pos"/></vertices>
      <triangles count="1"><input semantic="VERTEX" source="#g-verts" offset="0"/><p>0 1 2</p></triangles>
    </mesh></geometry>
  </library_geometries>
  <library_visual_scenes><visual_scene id="s">
    <node id="parent">
      <translate>100 0 0</translate>
      <node id="child">
        <translate>5 0 0</translate>
        <instance_geometry url="#g"/>
      </node>
    </node>
  </visual_scene></library_visual_scenes>
</COLLADA>"##;
        let mesh = DaeHandler.mesh("nested.dae", dae.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.bounds(), Some(([105.0, 0.0, 0.0], [106.0, 1.0, 0.0])));
    }

    #[test]
    fn instances_two_geometries_into_one_scene() {
        let dae = r##"<COLLADA version="1.4.1">
  <library_geometries>
    <geometry id="a"><mesh>
      <source id="a-pos"><float_array id="a-array" count="9">0 0 0 1 0 0 0 1 0</float_array></source>
      <vertices id="a-v"><input semantic="POSITION" source="#a-pos"/></vertices>
      <triangles count="1"><input semantic="VERTEX" source="#a-v" offset="0"/><p>0 1 2</p></triangles>
    </mesh></geometry>
    <geometry id="b"><mesh>
      <source id="b-pos"><float_array id="b-array" count="9">0 0 0 1 0 0 0 1 0</float_array></source>
      <vertices id="b-v"><input semantic="POSITION" source="#b-pos"/></vertices>
      <triangles count="1"><input semantic="VERTEX" source="#b-v" offset="0"/><p>0 1 2</p></triangles>
    </mesh></geometry>
  </library_geometries>
  <library_visual_scenes><visual_scene id="s">
    <node id="n"><instance_geometry url="#a"/><instance_geometry url="#b"/></node>
  </visual_scene></library_visual_scenes>
</COLLADA>"##;
        let mesh = DaeHandler.mesh("two.dae", dae.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.vertex_count(), 6);
    }

    #[test]
    fn renders_all_geometry_without_a_scene() {
        let dae = r##"<COLLADA version="1.4.1"><library_geometries>
  <geometry id="solo"><mesh>
    <source id="s-pos"><float_array id="s-array" count="9">0 0 0 1 0 0 0 1 0</float_array></source>
    <vertices id="s-v"><input semantic="POSITION" source="#s-pos"/></vertices>
    <triangles count="1"><input semantic="VERTEX" source="#s-v" offset="0"/><p>0 1 2</p></triangles>
  </mesh></geometry>
</library_geometries></COLLADA>"##;
        let mesh = DaeHandler.mesh("solo.dae", dae.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn rejects_input_without_a_collada_root() {
        let err = DaeHandler.metadata("nope.dae", b"<html>not collada</html>").unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "unexpected: {err:?}");
    }
}
