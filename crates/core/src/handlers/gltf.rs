//! glTF 2.0 (`.gltf` / `.glb`) handler.
//!
//! glTF is the Khronos open interchange format for 3D scenes. It has two
//! container flavours handled here:
//!
//! * `.gltf` — a UTF-8 JSON document. Binary geometry lives in one or more
//!   `buffers`, addressed either by an external relative `uri` or by an
//!   embedded `data:` URI (base64).
//! * `.glb` — a binary container: a 12-byte header followed by a JSON chunk and
//!   an optional `BIN` chunk that backs the first, URI-less buffer.
//!
//! The geometry is already triangles, so like STL/OBJ/PLY/3MF/COLLADA it maps
//! straight onto a [`Mesh`] with no external converter or WebAssembly kernel.
//! The JSON's `accessors`/`bufferViews` are decoded from the raw buffer bytes,
//! node hierarchies are composed (matrix or TRS), and triangle strips/fans are
//! expanded.
//!
//! Note on external buffers: a handler only receives the bytes of the one file
//! being previewed, so a `.gltf` whose `buffer.uri` points at a sibling `.bin`
//! cannot be resolved. Such files still list and report metadata; the mesh is
//! simply empty. Use `.glb` (or a `.gltf` with embedded `data:` buffers) for a
//! guaranteed self-contained preview.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::{Error, Mesh, Result};

use super::{basic_attributes, fmt_vec3, CadHandler, FileMetadata};

/// Guard against pathological/cyclic node graphs.
const MAX_DEPTH: usize = 64;

/// glTF `primitive.mode`: triangle list (the spec default).
const MODE_TRIANGLES: u32 = 4;
/// glTF `primitive.mode`: triangle strip.
const MODE_TRIANGLE_STRIP: u32 = 5;
/// glTF `primitive.mode`: triangle fan.
const MODE_TRIANGLE_FAN: u32 = 6;

/// Column-major 4×4 identity (glTF's matrix convention).
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// glTF/GLB handler: `.gltf` and `.glb`.
pub struct GltfHandler;

impl CadHandler for GltfHandler {
    fn name(&self) -> &'static str {
        "gltf"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["gltf", "glb"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let doc = parse(bytes)?;
        let gltf = &doc.gltf;
        let asset = &gltf.asset;
        let mut attributes = basic_attributes(path, bytes);
        attributes.insert("container".into(), doc.container.into());
        insert(&mut attributes, "version", &asset.version);
        insert(&mut attributes, "generator", &asset.generator);
        insert(&mut attributes, "copyright", &asset.copyright);
        insert(&mut attributes, "min_version", &asset.min_version);

        attributes.insert("scenes".into(), gltf.scenes.len().to_string());
        attributes.insert("nodes".into(), gltf.nodes.len().to_string());
        attributes.insert("meshes".into(), gltf.meshes.len().to_string());
        attributes.insert("materials".into(), gltf.materials.len().to_string());
        attributes.insert("images".into(), gltf.images.len().to_string());
        attributes.insert("animations".into(), gltf.animations.len().to_string());
        attributes.insert("accessors".into(), gltf.accessors.len().to_string());
        attributes.insert("buffer_views".into(), gltf.buffer_views.len().to_string());
        attributes.insert("buffers".into(), gltf.buffers.len().to_string());
        if !gltf.extensions_used.is_empty() {
            attributes.insert(
                "extensions_used".into(),
                gltf.extensions_used.join(", "),
            );
        }

        let unresolved = doc.unresolved_buffers();
        if unresolved > 0 {
            attributes.insert("unresolved_buffers".into(), unresolved.to_string());
            attributes.insert(
                "buffer_note".into(),
                "external buffers cannot be read from a single-file preview".into(),
            );
        }

        attributes.insert("vertices".into(), doc.mesh.vertex_count().to_string());
        attributes.insert("triangles".into(), doc.mesh.triangle_count().to_string());
        if let Some((min, max)) = doc.mesh.bounds() {
            attributes.insert("bounds_min".into(), fmt_vec3(min));
            attributes.insert("bounds_max".into(), fmt_vec3(max));
        }
        attributes.insert("volume".into(), format!("{:.4}", doc.mesh.volume()));

        Ok(FileMetadata {
            format: "gltf".into(),
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

/// The parsed document: summary counts plus the assembled mesh.
struct GltfDocument {
    gltf: Gltf,
    /// `"json"` for `.gltf`, `"binary"` for `.glb`.
    container: &'static str,
    /// One entry per `buffer`; `None` when the buffer's bytes are not available
    /// (an external relative `uri`).
    buffers: Vec<Option<Vec<u8>>>,
    mesh: Mesh,
}

impl GltfDocument {
    fn unresolved_buffers(&self) -> usize {
        self.buffers.iter().filter(|buffer| buffer.is_none()).count()
    }
}

/// The parts of a glTF JSON document OpenPDM reads. Unknown members are
/// ignored by serde.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Gltf {
    asset: Asset,
    /// Index of the default scene, if any.
    scene: Option<usize>,
    scenes: Vec<Scene>,
    nodes: Vec<Node>,
    meshes: Vec<MeshDef>,
    accessors: Vec<Accessor>,
    buffer_views: Vec<BufferView>,
    buffers: Vec<Buffer>,
    /// Counted for metadata only; the shapes are unused.
    materials: Vec<serde_json::Value>,
    images: Vec<serde_json::Value>,
    animations: Vec<serde_json::Value>,
    extensions_used: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Asset {
    version: String,
    generator: String,
    copyright: String,
    min_version: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Scene {
    nodes: Vec<usize>,
    #[allow(dead_code)]
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Node {
    mesh: Option<usize>,
    children: Vec<usize>,
    matrix: Option<Vec<f32>>,
    translation: Option<Vec<f32>>,
    rotation: Option<Vec<f32>>,
    scale: Option<Vec<f32>>,
    #[allow(dead_code)]
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MeshDef {
    primitives: Vec<Primitive>,
    #[allow(dead_code)]
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Primitive {
    attributes: BTreeMap<String, usize>,
    indices: Option<usize>,
    mode: Option<u32>,
    #[allow(dead_code)]
    material: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Accessor {
    buffer_view: Option<usize>,
    byte_offset: Option<usize>,
    component_type: Option<u32>,
    count: Option<usize>,
    #[serde(rename = "type")]
    kind: Option<String>,
    normalized: bool,
    /// Sparse accessors are unusual for POSITION/indices; their base data is
    /// still read, but the sparse override is not applied.
    #[allow(dead_code)]
    sparse: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct BufferView {
    buffer: Option<usize>,
    byte_offset: Option<usize>,
    byte_length: Option<usize>,
    byte_stride: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Buffer {
    byte_length: Option<usize>,
    uri: Option<String>,
}

/// Parse a `.gltf`/`.glb` file into a document summary and a triangle mesh.
fn parse(bytes: &[u8]) -> Result<GltfDocument> {
    let (json, binary, container) = split_container(bytes)?;
    let gltf: Gltf = serde_json::from_str(&json)
        .map_err(|e| Error::parse("gltf", format!("invalid glTF JSON: {e}")))?;

    // Validate that this is actually glTF, not arbitrary JSON.
    if gltf.asset.version.is_empty() && gltf.meshes.is_empty() && gltf.scenes.is_empty() {
        return Err(Error::parse("gltf", "missing `asset.version` (not a glTF document)"));
    }

    let buffers = resolve_buffers(&gltf, binary)?;
    let mesh = assemble(&gltf, &buffers);

    Ok(GltfDocument {
        gltf,
        container,
        buffers,
        mesh,
    })
}

/// Split raw bytes into `(JSON text, optional BIN chunk, container name)`.
///
/// A `.glb` is detected by its `glTF` magic regardless of file extension; a
/// `.gltf` is taken as UTF-8 JSON.
fn split_container(bytes: &[u8]) -> Result<(String, Option<Vec<u8>>, &'static str)> {
    if bytes.starts_with(b"glTF") {
        let (json, bin) = parse_glb(bytes)?;
        return Ok((json, bin, "binary"));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::parse("gltf", "document is neither GLB nor valid UTF-8 JSON"))?;
    Ok((text.to_string(), None, "json"))
}

/// Parse a GLB container: header, then length-prefixed chunks.
fn parse_glb(bytes: &[u8]) -> Result<(String, Option<Vec<u8>>)> {
    if bytes.len() < 12 {
        return Err(Error::parse("glb", "truncated GLB header"));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != 2 {
        return Err(Error::parse("glb", format!("unsupported GLB version {version}")));
    }
    let declared = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if declared > bytes.len() {
        return Err(Error::parse("glb", "GLB length exceeds the file size"));
    }

    let mut offset = 12usize;
    let end = declared.max(12).min(bytes.len());
    let mut json: Option<String> = None;
    let mut bin: Option<Vec<u8>> = None;

    while offset + 8 <= end {
        let length = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
        let start = offset + 8;
        let stop = start
            .checked_add(length)
            .ok_or_else(|| Error::parse("glb", "GLB chunk length overflow"))?;
        if stop > bytes.len() {
            return Err(Error::parse("glb", "truncated GLB chunk"));
        }
        let chunk = &bytes[start..stop];
        match kind {
            // `JSON`
            0x4E4F534A => {
                json = Some(
                    String::from_utf8_lossy(chunk).trim_end_matches('\0').to_string(),
                );
            }
            // `BIN\0`
            0x004E4942 => bin = Some(chunk.to_vec()),
            _ => {}
        }
        // Chunks are padded to a 4-byte boundary.
        offset = stop + (4 - (length % 4)) % 4;
    }

    let json = json.ok_or_else(|| Error::parse("glb", "no JSON chunk found"))?;
    Ok((json, bin))
}

/// Resolve the bytes backing each `buffer`.
///
/// * URI-less buffers use the GLB `BIN` chunk (buffer 0 by spec).
/// * `data:` URIs are base64-decoded.
/// * Any other URI is external and cannot be read from a single file, so it
///   resolves to `None`.
fn resolve_buffers(gltf: &Gltf, binary: Option<Vec<u8>>) -> Result<Vec<Option<Vec<u8>>>> {
    let mut bin = binary;
    gltf.buffers
        .iter()
        .map(|buffer| match buffer.uri.as_deref() {
            None => Ok(bin.take()),
            Some(uri) => Ok(decode_data_uri(uri)),
        })
        .collect()
}

/// Decode a `data:[<mediatype>][;base64],<data>` URI. Returns `None` for a
/// non-data URI (an external file) or malformed base64.
fn decode_data_uri(uri: &str) -> Option<Vec<u8>> {
    let rest = uri.strip_prefix("data:")?;
    let comma = rest.find(',')?;
    let (meta, data) = rest.split_at(comma);
    let data = &data[1..];
    if !meta.contains("base64") {
        // Percent-encoded text data; not used by glTF geometry buffers.
        return None;
    }
    base64_decode(data)
}

/// Assemble the mesh from the scene graph (falling back to every mesh when the
/// document has no usable node hierarchy).
fn assemble(gltf: &Gltf, buffers: &[Option<Vec<u8>>]) -> Mesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();

    let roots = scene_roots(gltf);
    if roots.is_empty() {
        for mesh in &gltf.meshes {
            append_mesh(gltf, buffers, mesh, IDENTITY, &mut positions, &mut indices);
        }
    } else {
        for root in roots {
            traverse(gltf, buffers, root, IDENTITY, 0, &mut positions, &mut indices);
        }
        // A scene whose nodes reference no mesh still has geometry worth
        // rendering, so fall back rather than showing an empty preview.
        if indices.is_empty() {
            for mesh in &gltf.meshes {
                append_mesh(gltf, buffers, mesh, IDENTITY, &mut positions, &mut indices);
            }
        }
    }

    Mesh::new(positions, indices)
}

/// The node indices a scene traversal should start from: the selected scene's
/// nodes, else every node that is not a child of another node.
fn scene_roots(gltf: &Gltf) -> Vec<usize> {
    if !gltf.scenes.is_empty() {
        let index = gltf.scene.unwrap_or(0).min(gltf.scenes.len() - 1);
        return gltf.scenes[index].nodes.clone();
    }

    let mut children = BTreeSet::new();
    for node in &gltf.nodes {
        children.extend(node.children.iter().copied());
    }
    gltf.nodes
        .iter()
        .enumerate()
        .map(|(index, _)| index)
        .filter(|index| !children.contains(index))
        .collect()
}

/// Walk a node subtree, composing transforms, and append its mesh geometry.
fn traverse(
    gltf: &Gltf,
    buffers: &[Option<Vec<u8>>],
    node_index: usize,
    parent: [f32; 16],
    depth: usize,
    positions: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(node) = gltf.nodes.get(node_index) else {
        return;
    };

    let world = mul(parent, local_transform(node));
    if let Some(mesh_index) = node.mesh {
        if let Some(mesh) = gltf.meshes.get(mesh_index) {
            append_mesh(gltf, buffers, mesh, world, positions, indices);
        }
    }
    for &child in &node.children {
        traverse(gltf, buffers, child, world, depth + 1, positions, indices);
    }
}

/// Append every primitive of a mesh, transformed by `transform`.
fn append_mesh(
    gltf: &Gltf,
    buffers: &[Option<Vec<u8>>],
    mesh: &MeshDef,
    transform: [f32; 16],
    positions: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    for primitive in &mesh.primitives {
        append_primitive(gltf, buffers, primitive, transform, positions, indices);
    }
}

/// Append one primitive: read its POSITION (and optional indices) accessors,
/// apply the primitive mode and transform, and merge into the output.
fn append_primitive(
    gltf: &Gltf,
    buffers: &[Option<Vec<u8>>],
    primitive: &Primitive,
    transform: [f32; 16],
    positions: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let Some(&position_accessor) = primitive.attributes.get("POSITION") else {
        return;
    };
    // POSITION is specified as FLOAT VEC3; anything else would misalign the
    // per-vertex stride, so skip it rather than emit garbage.
    if gltf
        .accessors
        .get(position_accessor)
        .and_then(|accessor| accessor.kind.as_deref())
        != Some("VEC3")
    {
        return;
    }
    let Ok(vertex_values) = read_accessor_f64(gltf, buffers, position_accessor) else {
        return;
    };
    let vertex_count = vertex_values.len() / 3;
    if vertex_count == 0 {
        return;
    }
    let vertices: Vec<f32> = vertex_values.iter().map(|&value| value as f32).collect();

    // Read the index accessor, or synthesise a sequential list for a
    // non-indexed primitive.
    let raw_indices = match primitive.indices {
        Some(accessor) => match read_accessor_u32(gltf, buffers, accessor) {
            Ok(values) => values,
            Err(_) => return,
        },
        None => (0..vertex_count as u32).collect(),
    };

    let triangles = triangles_of(primitive.mode.unwrap_or(MODE_TRIANGLES), &raw_indices);

    let base = (positions.len() / 3) as u32;
    for vertex in vertices.chunks_exact(3) {
        positions.extend_from_slice(&apply(&transform, vertex));
    }
    for triangle in triangles.chunks_exact(3) {
        if triangle.iter().any(|&index| index as usize >= vertex_count) {
            // Drop references to a missing vertex rather than hand the viewer
            // an out-of-range index.
            continue;
        }
        indices.push(base + triangle[0]);
        indices.push(base + triangle[1]);
        indices.push(base + triangle[2]);
    }
}

/// Expand a primitive's index list into triangles according to its mode.
/// Modes that carry no triangles (points and lines) yield nothing.
fn triangles_of(mode: u32, raw: &[u32]) -> Vec<u32> {
    match mode {
        MODE_TRIANGLES => raw.to_vec(),
        MODE_TRIANGLE_STRIP => {
            let mut out = Vec::new();
            for i in 0..raw.len().saturating_sub(2) {
                if i % 2 == 0 {
                    out.extend_from_slice(&[raw[i], raw[i + 1], raw[i + 2]]);
                } else {
                    out.extend_from_slice(&[raw[i + 1], raw[i], raw[i + 2]]);
                }
            }
            out
        }
        MODE_TRIANGLE_FAN => {
            let mut out = Vec::new();
            for i in 1..raw.len().saturating_sub(1) {
                out.extend_from_slice(&[raw[0], raw[i], raw[i + 1]]);
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Read an accessor as a flat `f64` array (all components per element).
fn read_accessor_f64(
    gltf: &Gltf,
    buffers: &[Option<Vec<u8>>],
    index: usize,
) -> Result<Vec<f64>> {
    let accessor = gltf
        .accessors
        .get(index)
        .ok_or_else(|| Error::parse("gltf", format!("accessor {index} is missing")))?;
    let layout = layout_of(gltf, buffers, accessor)?;

    // Cap the pre-allocation: a malformed `count` must not OOM us before the
    // reads fail on an exhausted buffer (each component is at least 1 byte).
    let capacity = layout
        .count
        .saturating_mul(layout.components)
        .min(layout.bytes.len());
    let mut values = Vec::with_capacity(capacity);
    for element in 0..layout.count {
        let start = layout.element_offset(element)?;
        for component in 0..layout.components {
            let at = start + component * layout.component_size;
            values.push(read_component(
                layout.bytes,
                at,
                layout.component_type,
                accessor.normalized,
            )?);
        }
    }
    Ok(values)
}

/// Read an accessor as a flat `u32` array (used for `SCALAR` indices).
fn read_accessor_u32(
    gltf: &Gltf,
    buffers: &[Option<Vec<u8>>],
    index: usize,
) -> Result<Vec<u32>> {
    let accessor = gltf
        .accessors
        .get(index)
        .ok_or_else(|| Error::parse("gltf", format!("accessor {index} is missing")))?;
    let layout = layout_of(gltf, buffers, accessor)?;

    let mut values = Vec::with_capacity(layout.count.min(layout.bytes.len()));
    for element in 0..layout.count {
        let at = layout.element_offset(element)?;
        let value = read_component(layout.bytes, at, layout.component_type, false)?;
        values.push(value.max(0.0) as u32);
    }
    Ok(values)
}

/// The resolved byte layout of an accessor's backing buffer.
struct Layout<'a> {
    bytes: &'a [u8],
    base: usize,
    count: usize,
    components: usize,
    component_size: usize,
    component_type: u32,
    stride: usize,
}

impl Layout<'_> {
    /// Byte offset of the start of element `index`, guarding against overflow
    /// from malformed `count`/`byteStride` values.
    fn element_offset(&self, index: usize) -> Result<usize> {
        index
            .checked_mul(self.stride)
            .and_then(|offset| offset.checked_add(self.base))
            .ok_or_else(|| Error::parse("gltf", "accessor offset overflow"))
    }
}

/// Resolve an accessor to its buffer, offset, element shape and stride.
fn layout_of<'a>(
    gltf: &'a Gltf,
    buffers: &'a [Option<Vec<u8>>],
    accessor: &Accessor,
) -> Result<Layout<'a>> {
    let view_index = accessor
        .buffer_view
        .ok_or_else(|| Error::parse("gltf", "accessor has no `bufferView`"))?;
    let view = gltf
        .buffer_views
        .get(view_index)
        .ok_or_else(|| Error::parse("gltf", format!("bufferView {view_index} is missing")))?;
    let buffer_index = view
        .buffer
        .ok_or_else(|| Error::parse("gltf", "bufferView has no `buffer`"))?;
    let bytes = buffers
        .get(buffer_index)
        .and_then(|buffer| buffer.as_deref())
        .ok_or_else(|| Error::parse("gltf", format!("buffer {buffer_index} is not available")))?;

    let component_type = accessor
        .component_type
        .ok_or_else(|| Error::parse("gltf", "accessor has no `componentType`"))?;
    let component_size = component_size(component_type)
        .ok_or_else(|| Error::parse("gltf", format!("unknown componentType {component_type}")))?;
    let kind = accessor
        .kind
        .as_deref()
        .ok_or_else(|| Error::parse("gltf", "accessor has no `type`"))?;
    let components = component_count(kind)
        .ok_or_else(|| Error::parse("gltf", format!("unknown accessor type `{kind}`")))?;
    let count = accessor.count.unwrap_or(0);

    let element_size = component_size * components;
    let base = view.byte_offset.unwrap_or(0) + accessor.byte_offset.unwrap_or(0);
    // A declared stride narrower than one element is invalid; fall back to the
    // tightly packed size so reads stay within each element.
    let stride = match view.byte_stride {
        Some(stride) if stride >= element_size => stride,
        _ => element_size,
    };

    Ok(Layout {
        bytes,
        base,
        count,
        components,
        component_size,
        component_type,
        stride,
    })
}

/// Read one scalar component at byte offset `at` and decode it to `f64`.
fn read_component(bytes: &[u8], at: usize, component_type: u32, normalized: bool) -> Result<f64> {
    let size = component_size(component_type)
        .ok_or_else(|| Error::parse("gltf", format!("unknown componentType {component_type}")))?;
    let end = at
        .checked_add(size)
        .ok_or_else(|| Error::parse("gltf", "accessor offset overflow"))?;
    let slice = bytes
        .get(at..end)
        .ok_or_else(|| Error::parse("gltf", "accessor reads past the end of its buffer"))?;
    let arr4 = || -> [u8; 4] { slice.try_into().unwrap() };
    let arr2 = || -> [u8; 2] { slice.try_into().unwrap() };
    let arr8 = || -> [u8; 8] { slice.try_into().unwrap() };

    let value = match component_type {
        5120 => slice[0] as i8 as f64,
        5121 => slice[0] as f64,
        5122 => i16::from_le_bytes(arr2()) as f64,
        5123 => u16::from_le_bytes(arr2()) as f64,
        5125 => u32::from_le_bytes(arr4()) as f64,
        5126 => f32::from_le_bytes(arr4()) as f64,
        5130 => f64::from_le_bytes(arr8()), // KHR_double (rare)
        other => {
            return Err(Error::parse(
                "gltf",
                format!("unsupported componentType {other}"),
            ))
        }
    };

    if normalized {
        Ok(normalize(component_type, value))
    } else {
        Ok(value)
    }
}

/// Convert a normalized integer component to its `[0,1]`/`[-1,1]` float range.
fn normalize(component_type: u32, value: f64) -> f64 {
    match component_type {
        5120 => (value / 127.0).max(-1.0),
        5121 => value / 255.0,
        5122 => (value / 32767.0).max(-1.0),
        5123 => value / 65535.0,
        5125 => value / 4294967295.0,
        _ => value,
    }
}

/// Byte width of a glTF component type.
fn component_size(component_type: u32) -> Option<usize> {
    Some(match component_type {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        5130 => 8,
        _ => return None,
    })
}

/// Number of components in a glTF accessor type.
fn component_count(kind: &str) -> Option<usize> {
    Some(match kind {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT2" => 4,
        "MAT3" => 9,
        "MAT4" => 16,
        _ => return None,
    })
}

/// The local transform of a node, from an explicit `matrix` or from TRS.
fn local_transform(node: &Node) -> [f32; 16] {
    if let Some(matrix) = &node.matrix {
        if matrix.len() == 16 {
            let mut out = [0f32; 16];
            out.copy_from_slice(matrix);
            return out;
        }
    }

    let translation = vec3(&node.translation, 0.0);
    let scale = vec3(&node.scale, 1.0);
    let [qx, qy, qz, qw] = quaternion(node.rotation.as_deref());

    // glTF composes T * R * S (column-major, column vectors).
    let mut t = IDENTITY;
    t[12] = translation[0];
    t[13] = translation[1];
    t[14] = translation[2];

    let r = quaternion_matrix(qx, qy, qz, qw);

    let mut s = IDENTITY;
    s[0] = scale[0];
    s[5] = scale[1];
    s[10] = scale[2];

    mul(mul(t, r), s)
}

fn vec3(values: &Option<Vec<f32>>, default: f32) -> [f32; 3] {
    let values = values.as_deref().unwrap_or_default();
    [
        values.first().copied().unwrap_or(default),
        values.get(1).copied().unwrap_or(default),
        values.get(2).copied().unwrap_or(default),
    ]
}

/// Read `[x, y, z, w]` from a glTF quaternion, defaulting to identity.
fn quaternion(values: Option<&[f32]>) -> [f32; 4] {
    match values {
        Some(values) if values.len() == 4 => [values[0], values[1], values[2], values[3]],
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

/// Column-major rotation matrix from a glTF `[x, y, z, w]` quaternion.
fn quaternion_matrix(x: f32, y: f32, z: f32, w: f32) -> [f32; 16] {
    let norm = (x * x + y * y + z * z + w * w).sqrt();
    if norm == 0.0 {
        return IDENTITY;
    }
    let (x, y, z, w) = (x / norm, y / norm, z / norm, w / norm);
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + w * z),
        2.0 * (x * z - w * y),
        0.0,
        2.0 * (x * y - w * z),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + w * x),
        0.0,
        2.0 * (x * z + w * y),
        2.0 * (y * z - w * x),
        1.0 - 2.0 * (x * x + y * y),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

/// Column-major matrix product: the result applies `b`, then `a`.
fn mul(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = sum;
        }
    }
    out
}

/// Transform a point by a column-major matrix.
fn apply(m: &[f32; 16], p: &[f32]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// Decode standard (or URL-safe) base64, ignoring whitespace and padding.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u8> {
        Some(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    }

    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for &byte in input.as_bytes() {
        if byte.is_ascii_whitespace() || byte == b'=' {
            continue;
        }
        let digit = value(byte)?;
        buffer = (buffer << 6) | digit as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal `.gltf` with a base64-encoded 3-vertex, 1-triangle buffer.
    fn data_uri_gltf() -> String {
        // positions: (0,0,0) (1,0,0) (0,1,0); indices: 0,1,2 (u16).
        let mut bin = Vec::new();
        for value in [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
        for index in [0u16, 1, 2] {
            bin.extend_from_slice(&index.to_le_bytes());
        }
        // Pad to a 4-byte boundary (already 40 bytes) and encode.
        let encoded = base64_encode(&bin);
        format!(
            r#"{{
  "asset": {{ "version": "2.0", "generator": "OpenPDM test" }},
  "scene": 0,
  "scenes": [ {{ "nodes": [0] }} ],
  "nodes": [ {{ "mesh": 0 }} ],
  "meshes": [ {{ "primitives": [ {{ "attributes": {{ "POSITION": 0 }}, "indices": 1 }} ] }} ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" }},
    {{ "bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR" }}
  ],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": 0, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": 36, "byteLength": 6 }}
  ],
  "buffers": [ {{ "byteLength": 42, "uri": "data:application/octet-stream;base64,{encoded}" }} ]
}}"#
        )
    }

    /// Encode for building test fixtures (mirrors the decoder).
    fn base64_encode(input: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in input.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
            out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
            if chunk.len() > 1 {
                out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(ALPHABET[(n & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }

    /// Build a `.glb` from a JSON string and an optional binary chunk.
    fn build_glb(json: &str, bin: &[u8]) -> Vec<u8> {
        let mut json_bytes = json.as_bytes().to_vec();
        while !json_bytes.len().is_multiple_of(4) {
            json_bytes.push(b' ');
        }
        let mut bin_bytes = bin.to_vec();
        while !bin_bytes.len().is_multiple_of(4) {
            bin_bytes.push(0);
        }

        let mut body = Vec::new();
        // JSON chunk.
        body.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        body.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        body.extend_from_slice(&json_bytes);
        // BIN chunk, if present.
        if !bin_bytes.is_empty() {
            body.extend_from_slice(&(bin_bytes.len() as u32).to_le_bytes());
            body.extend_from_slice(&0x004E4942u32.to_le_bytes());
            body.extend_from_slice(&bin_bytes);
        }

        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&((12 + body.len()) as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// The binary payload shared by the GLB fixtures: a unit triangle.
    fn triangle_bin() -> Vec<u8> {
        let mut bin = Vec::new();
        for value in [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
        for index in [0u16, 1, 2] {
            bin.extend_from_slice(&index.to_le_bytes());
        }
        bin
    }

    const GLB_JSON: &str = r#"{
  "asset": { "version": "2.0", "generator": "OpenPDM test" },
  "scene": 0,
  "scenes": [ { "nodes": [0] } ],
  "nodes": [ { "mesh": 0 } ],
  "meshes": [ { "primitives": [ { "attributes": { "POSITION": 0 }, "indices": 1 } ] } ],
  "accessors": [
    { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" },
    { "bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR" }
  ],
  "bufferViews": [
    { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
    { "buffer": 0, "byteOffset": 36, "byteLength": 6 }
  ],
  "buffers": [ { "byteLength": 42 } ]
}"#;

    #[test]
    fn parses_glb_triangle() {
        let bytes = build_glb(GLB_JSON, &triangle_bin());
        let mesh = GltfHandler.mesh("tri.glb", &bytes).unwrap().unwrap();
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.bounds(), Some(([0.0, 0.0, 0.0], [1.0, 1.0, 0.0])));
    }

    #[test]
    fn parses_gltf_with_embedded_buffer() {
        let json = data_uri_gltf();
        let mesh = GltfHandler.mesh("tri.gltf", json.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn extracts_metadata() {
        let bytes = build_glb(GLB_JSON, &triangle_bin());
        let meta = GltfHandler.metadata("tri.glb", &bytes).unwrap();
        assert_eq!(meta.format, "gltf");
        assert_eq!(meta.attributes.get("container").map(String::as_str), Some("binary"));
        assert_eq!(meta.attributes.get("version").map(String::as_str), Some("2.0"));
        assert_eq!(
            meta.attributes.get("generator").map(String::as_str),
            Some("OpenPDM test")
        );
        assert_eq!(meta.attributes.get("scenes").map(String::as_str), Some("1"));
        assert_eq!(meta.attributes.get("nodes").map(String::as_str), Some("1"));
        assert_eq!(meta.attributes.get("meshes").map(String::as_str), Some("1"));
        assert_eq!(meta.attributes.get("triangles").map(String::as_str), Some("1"));
    }

    #[test]
    fn applies_node_translation() {
        let json = GLB_JSON.replace(
            "\"nodes\": [ { \"mesh\": 0 } ]",
            "\"nodes\": [ { \"mesh\": 0, \"translation\": [10.0, 20.0, 30.0] } ]",
        );
        let bytes = build_glb(&json, &triangle_bin());
        let mesh = GltfHandler.mesh("moved.glb", &bytes).unwrap().unwrap();
        assert_eq!(mesh.bounds(), Some(([10.0, 20.0, 30.0], [11.0, 21.0, 30.0])));
    }

    #[test]
    fn composes_nested_transforms() {
        let json = r#"{
  "asset": { "version": "2.0" },
  "scenes": [ { "nodes": [0] } ],
  "nodes": [
    { "translation": [100.0, 0.0, 0.0], "children": [1] },
    { "mesh": 0, "translation": [5.0, 0.0, 0.0] }
  ],
  "meshes": [ { "primitives": [ { "attributes": { "POSITION": 0 } } ] } ],
  "accessors": [ { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" } ],
  "bufferViews": [ { "buffer": 0, "byteOffset": 0, "byteLength": 36 } ],
  "buffers": [ { "byteLength": 36 } ]
}"#;
        let bytes = build_glb(json, &triangle_bin()[..36]);
        let mesh = GltfHandler.mesh("nested.glb", &bytes).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.bounds(), Some(([105.0, 0.0, 0.0], [106.0, 1.0, 0.0])));
    }

    #[test]
    fn expands_triangle_strip() {
        // A strip of 4 vertices -> 2 triangles.
        let mut bin = Vec::new();
        for value in [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
        let json = r#"{
  "asset": { "version": "2.0" },
  "scenes": [ { "nodes": [0] } ],
  "nodes": [ { "mesh": 0 } ],
  "meshes": [ { "primitives": [ { "attributes": { "POSITION": 0 }, "mode": 5 } ] } ],
  "accessors": [ { "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3" } ],
  "bufferViews": [ { "buffer": 0, "byteOffset": 0, "byteLength": 48 } ],
  "buffers": [ { "byteLength": 48 } ]
}"#;
        let bytes = build_glb(json, &bin);
        let mesh = GltfHandler.mesh("strip.glb", &bytes).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 2);
    }

    #[test]
    fn skips_line_primitives() {
        let json = GLB_JSON.replace("\"indices\": 1", "\"indices\": 1, \"mode\": 1");
        let bytes = build_glb(&json, &triangle_bin());
        let mesh = GltfHandler.mesh("lines.glb", &bytes).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 0);
    }

    #[test]
    fn reports_unresolved_external_buffers() {
        let json = GLB_JSON.replace("\"byteLength\": 42", "\"byteLength\": 42, \"uri\": \"tri.bin\"");
        let meta = GltfHandler.metadata("ext.gltf", json.as_bytes()).unwrap();
        assert_eq!(
            meta.attributes.get("unresolved_buffers").map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn rejects_non_gltf_json() {
        let err = GltfHandler.metadata("nope.gltf", b"{ \"hello\": true }").unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "unexpected: {err:?}");
    }

    #[test]
    fn rejects_malformed_glb() {
        let err = GltfHandler
            .mesh("bad.glb", b"glTF\x02\x00\x00\x00")
            .unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "unexpected: {err:?}");
    }
}
