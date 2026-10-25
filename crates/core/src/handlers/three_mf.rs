//! 3MF (3D Manufacturing Format) handler.
//!
//! A `.3mf` file is an OPC package — a ZIP container. The 3D payload is a model
//! part (conventionally `3D/3dmodel.model`) referenced from `_rels/.rels`, an
//! XML document that declares the unit, package metadata, a `<resources>`
//! section of reusable objects (each a triangle `<mesh>` and/or a set of
//! `<component>` references) and a `<build>` section placing objects into the
//! scene, each with an optional 4×3 `transform`.
//!
//! The geometry is already triangles, so like STL/OBJ/PLY this maps straight
//! onto a [`Mesh`] with no external converter or WebAssembly kernel. Build items
//! and nested components are resolved and their transforms composed, so an
//! assembly renders in place rather than with every object stacked at the
//! origin.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::{Error, Mesh, Result};

use super::{basic_attributes, fmt_vec3, CadHandler, FileMetadata};

/// Relationship type suffix identifying the 3D model part in `_rels/.rels`.
const MODEL_RELATIONSHIP_SUFFIX: &str = "/3dmodel";

/// Row-vector 4×3 identity transform (the implicit fourth column is `0 0 0 1`).
const IDENTITY: [f32; 12] = [
    1.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, //
    0.0, 0.0, 1.0, //
    0.0, 0.0, 0.0,
];

/// Guard against pathological/cyclic component graphs.
const MAX_DEPTH: usize = 32;

/// 3MF handler: `.3mf`.
pub struct ThreeMfHandler;

impl CadHandler for ThreeMfHandler {
    fn name(&self) -> &'static str {
        "3mf"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["3mf"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let doc = parse(bytes)?;
        let mut attributes = basic_attributes(path, bytes);
        attributes.insert("container".into(), "zip".into());
        if let Some(unit) = &doc.unit {
            attributes.insert("unit".into(), unit.clone());
        }
        if let Some(language) = &doc.language {
            attributes.insert("language".into(), language.clone());
        }
        for (name, value) in &doc.metadata {
            attributes.insert(name.to_ascii_lowercase(), value.clone());
        }
        attributes.insert("objects".into(), doc.object_count.to_string());
        attributes.insert("build_items".into(), doc.build_count.to_string());
        attributes.insert("vertices".into(), doc.mesh.vertex_count().to_string());
        attributes.insert("triangles".into(), doc.mesh.triangle_count().to_string());
        if let Some((min, max)) = doc.mesh.bounds() {
            attributes.insert("bounds_min".into(), fmt_vec3(min));
            attributes.insert("bounds_max".into(), fmt_vec3(max));
        }
        attributes.insert("volume".into(), format!("{:.4}", doc.mesh.volume()));
        Ok(FileMetadata {
            format: "3mf".into(),
            attributes,
        })
    }

    fn mesh(&self, _path: &str, bytes: &[u8]) -> Result<Option<Mesh>> {
        Ok(Some(parse(bytes)?.mesh))
    }
}

/// A 3MF package reduced to the summary and assembled mesh a handler needs.
struct ThreeMfDocument {
    unit: Option<String>,
    language: Option<String>,
    metadata: Vec<(String, String)>,
    object_count: usize,
    build_count: usize,
    mesh: Mesh,
}

/// A `<resources><object>`: reusable local geometry and/or component references.
#[derive(Default, Clone)]
struct Object {
    id: String,
    positions: Vec<f32>,
    indices: Vec<u32>,
    components: Vec<Component>,
}

/// A `<component>` reference to another object, with an optional transform.
#[derive(Clone)]
struct Component {
    object_id: String,
    transform: Option<[f32; 12]>,
}

/// A `<build><item>` placement of an object, with an optional transform.
#[derive(Clone)]
struct BuildItem {
    object_id: String,
    transform: Option<[f32; 12]>,
}

/// The parsed model part before build resolution.
#[derive(Default)]
struct Model {
    unit: Option<String>,
    language: Option<String>,
    metadata: Vec<(String, String)>,
    objects: Vec<Object>,
    build: Vec<BuildItem>,
}

/// Read and fully parse a `.3mf` package.
fn parse(bytes: &[u8]) -> Result<ThreeMfDocument> {
    let mut archive = open_archive(bytes)?;
    let model_part = find_model_part(&mut archive)?;
    let xml = read_entry(&mut archive, &model_part)?.ok_or_else(|| {
        Error::parse("3mf", format!("model part `{model_part}` is missing"))
    })?;
    let model = parse_model(&xml)?;
    let mesh = assemble(&model);

    Ok(ThreeMfDocument {
        unit: model.unit,
        language: model.language,
        metadata: model.metadata,
        object_count: model.objects.len(),
        build_count: model.build.len(),
        mesh,
    })
}

/// Resolve the build items (or, absent a build, every mesh object) into a single
/// mesh, composing component/item transforms.
fn assemble(model: &Model) -> Mesh {
    let by_id: BTreeMap<&str, &Object> = model
        .objects
        .iter()
        .map(|object| (object.id.as_str(), object))
        .collect();

    let items: Vec<(&str, Option<[f32; 12]>)> = if model.build.is_empty() {
        model
            .objects
            .iter()
            .filter(|object| !object.indices.is_empty())
            .map(|object| (object.id.as_str(), None))
            .collect()
    } else {
        model
            .build
            .iter()
            .map(|item| (item.object_id.as_str(), item.transform))
            .collect()
    };

    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for (id, transform) in items {
        append_object(&by_id, id, transform, 0, &mut positions, &mut indices);
    }
    Mesh::new(positions, indices)
}

/// Append `id`'s geometry (and, recursively, its components) to the mesh.
fn append_object(
    by_id: &BTreeMap<&str, &Object>,
    id: &str,
    transform: Option<[f32; 12]>,
    depth: usize,
    positions: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(object) = by_id.get(id) else {
        return;
    };

    if !object.indices.is_empty() {
        let local_vertices = (object.positions.len() / 3) as u32;
        let base = (positions.len() / 3) as u32;
        match transform {
            Some(matrix) => {
                for vertex in object.positions.chunks_exact(3) {
                    let point = apply(&matrix, vertex);
                    positions.extend_from_slice(&point);
                }
            }
            None => positions.extend_from_slice(&object.positions),
        }
        for triangle in object.indices.chunks_exact(3) {
            // Drop triangles that reference a missing vertex rather than
            // handing the viewer an out-of-range index.
            if triangle.iter().any(|&index| index >= local_vertices) {
                continue;
            }
            indices.push(base + triangle[0]);
            indices.push(base + triangle[1]);
            indices.push(base + triangle[2]);
        }
    }

    for component in &object.components {
        // A component's transform applies to the child first, then the parent's.
        let child = compose(
            component.transform.unwrap_or(IDENTITY),
            transform.unwrap_or(IDENTITY),
        );
        append_object(
            by_id,
            &component.object_id,
            Some(child),
            depth + 1,
            positions,
            indices,
        );
    }
}

/// Apply a row-vector 4×3 transform to a point.
fn apply(m: &[f32; 12], p: &[f32]) -> [f32; 3] {
    [
        p[0] * m[0] + p[1] * m[3] + p[2] * m[6] + m[9],
        p[0] * m[1] + p[1] * m[4] + p[2] * m[7] + m[10],
        p[0] * m[2] + p[1] * m[5] + p[2] * m[8] + m[11],
    ]
}

/// Compose two row-vector 4×3 transforms: the result applies `a`, then `b`.
fn compose(a: [f32; 12], b: [f32; 12]) -> [f32; 12] {
    let mut out = [0f32; 12];
    for row in 0..3 {
        for col in 0..3 {
            out[row * 3 + col] = a[row * 3] * b[col]
                + a[row * 3 + 1] * b[3 + col]
                + a[row * 3 + 2] * b[6 + col];
        }
    }
    for col in 0..3 {
        out[9 + col] =
            b[9 + col] + a[9] * b[col] + a[10] * b[3 + col] + a[11] * b[6 + col];
    }
    out
}

/// Parse the model-part XML into objects, build items and metadata.
fn parse_model(xml: &[u8]) -> Result<Model> {
    let text = String::from_utf8_lossy(xml);
    let mut reader = Reader::from_str(&text);
    let mut buf = Vec::new();

    let mut model = Model::default();
    let mut current_object: Option<usize> = None;
    let mut in_vertices = false;
    let mut in_triangles = false;
    let mut metadata_name: Option<String> = None;
    let mut metadata_value = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(Error::parse("3mf", format!("malformed model XML: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"model" => {
                    model.unit = attr(&e, b"unit");
                    model.language = attr(&e, b"lang");
                }
                b"metadata" => {
                    metadata_name = attr(&e, b"name");
                    metadata_value.clear();
                }
                b"object" => push_object(&mut model, &e, &mut current_object),
                b"vertices" => in_vertices = true,
                b"triangles" => in_triangles = true,
                b"vertex" if in_vertices => read_vertex(&e, &mut model, current_object),
                b"triangle" if in_triangles => read_triangle(&e, &mut model, current_object),
                b"component" => read_component(&e, &mut model, current_object),
                b"item" => read_item(&e, &mut model),
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"object" => {
                    push_object(&mut model, &e, &mut current_object);
                    current_object = None;
                }
                b"vertex" if in_vertices => read_vertex(&e, &mut model, current_object),
                b"triangle" if in_triangles => read_triangle(&e, &mut model, current_object),
                b"component" => read_component(&e, &mut model, current_object),
                b"item" => read_item(&e, &mut model),
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if metadata_name.is_some() {
                    if let Ok(value) = t.unescape() {
                        metadata_value.push_str(&value);
                    }
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"metadata" => {
                    if let Some(name) = metadata_name.take() {
                        let value = metadata_value.trim().to_string();
                        if !value.is_empty() {
                            model.metadata.push((name, value));
                        }
                    }
                    metadata_value.clear();
                }
                b"object" => current_object = None,
                b"vertices" => in_vertices = false,
                b"triangles" => in_triangles = false,
                _ => {}
            },
            _ => {}
        }
        buf.clear();
    }

    Ok(model)
}

fn push_object(model: &mut Model, e: &BytesStart<'_>, current: &mut Option<usize>) {
    model.objects.push(Object {
        id: attr(e, b"id").unwrap_or_default(),
        ..Default::default()
    });
    *current = Some(model.objects.len() - 1);
}

fn read_vertex(e: &BytesStart<'_>, model: &mut Model, current: Option<usize>) {
    let Some(index) = current else { return };
    let object = &mut model.objects[index];
    object.positions.extend_from_slice(&[
        number(&attr(e, b"x")),
        number(&attr(e, b"y")),
        number(&attr(e, b"z")),
    ]);
}

fn read_triangle(e: &BytesStart<'_>, model: &mut Model, current: Option<usize>) {
    let Some(index) = current else { return };
    let object = &mut model.objects[index];
    // Indices are validated later, once the vertex count is known.
    for key in [b"v1".as_slice(), b"v2".as_slice(), b"v3".as_slice()] {
        object
            .indices
            .push(attr(e, key).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0));
    }
}

fn read_component(e: &BytesStart<'_>, model: &mut Model, current: Option<usize>) {
    let Some(index) = current else { return };
    let component = Component {
        object_id: attr(e, b"objectid").unwrap_or_default(),
        transform: attr(e, b"transform").as_deref().and_then(parse_transform),
    };
    model.objects[index].components.push(component);
}

fn read_item(e: &BytesStart<'_>, model: &mut Model) {
    model.build.push(BuildItem {
        object_id: attr(e, b"objectid").unwrap_or_default(),
        transform: attr(e, b"transform").as_deref().and_then(parse_transform),
    });
}

/// Parse a 3MF `transform`: 12 whitespace-separated numbers (a row-vector 4×3
/// matrix). Returns `None` for anything malformed, which degrades to identity.
fn parse_transform(raw: &str) -> Option<[f32; 12]> {
    let values: Vec<f32> = raw
        .split_whitespace()
        .map(str::parse::<f32>)
        .collect::<std::result::Result<Vec<f32>, _>>()
        .ok()?;
    values.try_into().ok()
}

/// Read an XML attribute by local name (namespace-prefix agnostic).
fn attr(element: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == key)
        .and_then(|attribute| attribute.unescape_value().ok().map(|value| value.into_owned()))
}

/// Parse an optional coordinate string, defaulting to `0.0`.
fn number(value: &Option<String>) -> f32 {
    value.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0.0)
}

/// Locate the 3D model part, preferring the `_rels/.rels` relationship and
/// falling back to the first `*.model` entry.
fn find_model_part(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<String> {
    if let Some(rels) = read_entry(archive, "_rels/.rels")? {
        if let Some(target) = model_relationship_target(&rels) {
            return Ok(target.trim_start_matches('/').to_string());
        }
    }
    for name in entry_names(archive)? {
        if name.to_ascii_lowercase().ends_with(".model") {
            return Ok(name);
        }
    }
    Err(Error::parse("3mf", "no 3D model part found in package"))
}

/// The `Target` of the relationship that points at the 3D model part.
fn model_relationship_target(xml: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(xml);
    let mut reader = Reader::from_str(&text);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                if e.local_name().as_ref() == b"Relationship" {
                    let ty = attr(&e, b"Type").unwrap_or_default();
                    if ty.ends_with(MODEL_RELATIONSHIP_SUFFIX) {
                        if let Some(target) = attr(&e, b"Target") {
                            return Some(target);
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

fn open_archive(bytes: &[u8]) -> Result<ZipArchive<Cursor<&[u8]>>> {
    ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| Error::parse("3mf", format!("not a valid 3MF/ZIP package: {e}")))
}

fn entry_names(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Vec<String>> {
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| Error::parse("3mf", format!("corrupt ZIP directory: {e}")))?;
        names.push(entry.name().to_string());
    }
    Ok(names)
}

fn read_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>> {
    match archive.by_name(name) {
        Ok(mut entry) => {
            let mut out = Vec::new();
            entry
                .read_to_end(&mut out)
                .map_err(|e| Error::parse("3mf", format!("reading {name}: {e}")))?;
            Ok(Some(out))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(Error::parse("3mf", format!("reading {name}: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn build_3mf(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    /// A single-object model: one triangle, `millimeter`, with a title.
    const SIMPLE_MODEL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xml:lang="en-US"
       xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <metadata name="Title">Bracket</metadata>
  <metadata name="Designer">Ada</metadata>
  <resources>
    <object id="1" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0" />
          <vertex x="1" y="0" z="0" />
          <vertex x="0" y="1" z="0" />
        </vertices>
        <triangles>
          <triangle v1="0" v2="1" v3="2" />
        </triangles>
      </mesh>
    </object>
  </resources>
  <build>
    <item objectid="1" />
  </build>
</model>"#;

    #[test]
    fn parses_simple_model() {
        let bytes = build_3mf(&[("3D/3dmodel.model", SIMPLE_MODEL)]);
        let mesh = ThreeMfHandler.mesh("part.3mf", &bytes).unwrap().unwrap();
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.bounds(), Some(([0.0, 0.0, 0.0], [1.0, 1.0, 0.0])));
    }

    #[test]
    fn extracts_metadata() {
        let bytes = build_3mf(&[("3D/3dmodel.model", SIMPLE_MODEL)]);
        let meta = ThreeMfHandler.metadata("part.3mf", &bytes).unwrap();
        assert_eq!(meta.format, "3mf");
        assert_eq!(meta.attributes.get("unit").map(String::as_str), Some("millimeter"));
        assert_eq!(meta.attributes.get("title").map(String::as_str), Some("Bracket"));
        assert_eq!(meta.attributes.get("designer").map(String::as_str), Some("Ada"));
        assert_eq!(meta.attributes.get("objects").map(String::as_str), Some("1"));
        assert_eq!(meta.attributes.get("build_items").map(String::as_str), Some("1"));
        assert_eq!(meta.attributes.get("triangles").map(String::as_str), Some("1"));
    }

    #[test]
    fn applies_build_item_transform() {
        let model = r#"<model unit="millimeter">
  <resources>
    <object id="1"><mesh>
      <vertices>
        <vertex x="0" y="0" z="0" /><vertex x="1" y="0" z="0" /><vertex x="0" y="1" z="0" />
      </vertices>
      <triangles><triangle v1="0" v2="1" v3="2" /></triangles>
    </mesh></object>
  </resources>
  <build><item objectid="1" transform="1 0 0 0 1 0 0 0 1 10 20 30" /></build>
</model>"#;
        let bytes = build_3mf(&[("3D/3dmodel.model", model)]);
        let mesh = ThreeMfHandler.mesh("moved.3mf", &bytes).unwrap().unwrap();
        assert_eq!(mesh.bounds(), Some(([10.0, 20.0, 30.0], [11.0, 21.0, 30.0])));
    }

    #[test]
    fn resolves_components_with_transforms() {
        // Object 1 is a container that places object 2 twice; the build only
        // references object 1.
        let model = r#"<model unit="millimeter">
  <resources>
    <object id="1">
      <components>
        <component objectid="2" />
        <component objectid="2" transform="1 0 0 0 1 0 0 0 1 5 0 0" />
      </components>
    </object>
    <object id="2"><mesh>
      <vertices>
        <vertex x="0" y="0" z="0" /><vertex x="1" y="0" z="0" /><vertex x="0" y="1" z="0" />
      </vertices>
      <triangles><triangle v1="0" v2="1" v3="2" /></triangles>
    </mesh></object>
  </resources>
  <build><item objectid="1" /></build>
</model>"#;
        let bytes = build_3mf(&[("3D/3dmodel.model", model)]);
        let mesh = ThreeMfHandler.mesh("asm.3mf", &bytes).unwrap().unwrap();
        // Two placements of the triangle.
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.bounds(), Some(([0.0, 0.0, 0.0], [6.0, 1.0, 0.0])));
    }

    #[test]
    fn uses_rels_to_find_the_model_part() {
        let rels = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rel0" Target="/3D/3dmodel.model"
    Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" />
</Relationships>"#;
        let bytes = build_3mf(&[
            ("_rels/.rels", rels),
            ("3D/3dmodel.model", SIMPLE_MODEL),
        ]);
        let mesh = ThreeMfHandler.mesh("part.3mf", &bytes).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn renders_all_mesh_objects_without_a_build_section() {
        let model = r#"<model unit="millimeter">
  <resources>
    <object id="1"><mesh>
      <vertices>
        <vertex x="0" y="0" z="0" /><vertex x="1" y="0" z="0" /><vertex x="0" y="1" z="0" />
      </vertices>
      <triangles><triangle v1="0" v2="1" v3="2" /></triangles>
    </mesh></object>
    <object id="2"><mesh>
      <vertices>
        <vertex x="0" y="0" z="0" /><vertex x="1" y="0" z="0" /><vertex x="0" y="1" z="0" />
      </vertices>
      <triangles><triangle v1="0" v2="1" v3="2" /></triangles>
    </mesh></object>
  </resources>
</model>"#;
        let bytes = build_3mf(&[("3D/3dmodel.model", model)]);
        let mesh = ThreeMfHandler.mesh("all.3mf", &bytes).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 2);
    }

    #[test]
    fn rejects_a_non_zip_container() {
        let err = ThreeMfHandler.metadata("nope.3mf", b"not a zip").unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "unexpected: {err:?}");
    }

    #[test]
    fn rejects_a_package_without_a_model_part() {
        let bytes = build_3mf(&[("readme.txt", "hello")]);
        let err = ThreeMfHandler.mesh("empty.3mf", &bytes).unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "unexpected: {err:?}");
    }
}
