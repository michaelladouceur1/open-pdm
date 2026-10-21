//! FreeCAD document (`.FCStd`) handler.
//!
//! An `.FCStd` file is a ZIP container. Two entries matter to us:
//!
//! * `Document.xml` — the manifest describing the document (label, author,
//!   timestamps) and every object plus its properties, including which `.brp`
//!   file holds each object's exact shape.
//! * `*.brp` — OpenCASCADE B-rep payloads, one per shape-bearing object.
//!
//! FreeCAD stores exact boundary representation, not triangles, so the core
//! cannot produce a [`Mesh`] itself. Instead [`FcstdHandler::shape_payloads`]
//! hands the raw B-rep to the desktop viewer, which tessellates it with
//! `occt-import-js` (OpenCASCADE in WebAssembly). The metadata advertises this
//! via `tessellation = brep` so the UI knows to ask for the payloads.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::{Error, Result};

use super::{basic_attributes, CadHandler, FileMetadata, ShapePayload};

/// `Document.xml` entry name inside the archive.
const DOCUMENT_XML: &str = "Document.xml";

/// FreeCAD `.FCStd` handler.
pub struct FcstdHandler;

impl CadHandler for FcstdHandler {
    fn name(&self) -> &'static str {
        "fcstd"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["fcstd"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let mut archive = open_archive(bytes)?;
        let entries = entry_names(&mut archive)?;

        let mut attributes = basic_attributes(path, bytes);
        attributes.insert("container".into(), "zip".into());
        attributes.insert("entries".into(), entries.len().to_string());

        let summary = match read_entry(&mut archive, DOCUMENT_XML)? {
            Some(xml) => parse_document(&xml)?,
            None => DocumentSummary::default(),
        };

        for (key, value) in &summary.document {
            if !value.is_empty() {
                attributes.insert(key.to_ascii_lowercase(), value.clone());
            }
        }
        if let Some(v) = &summary.schema_version {
            attributes.insert("schema_version".into(), v.clone());
        }
        if let Some(v) = &summary.file_version {
            attributes.insert("file_version".into(), v.clone());
        }
        if let Some(v) = &summary.program_version {
            attributes.insert("program_version".into(), v.clone());
        }

        attributes.insert(
            "object_count".into(),
            summary.object_types.len().to_string(),
        );
        if !summary.object_types.is_empty() {
            attributes.insert("object_types".into(), summarise_object_types(&summary));
        }

        // Shape files are discovered from the manifest, falling back to the
        // archive listing when the document is missing/unparseable.
        let mut shape_files: Vec<String> = summary
            .object_shape_files
            .iter()
            .map(|(_, file)| file.clone())
            .collect();
        shape_files.sort();
        shape_files.dedup();
        let shape_files: Vec<String> = if shape_files.is_empty() {
            entries
                .iter()
                .filter(|n| is_shape_path(n))
                .cloned()
                .collect()
        } else {
            shape_files
        };
        attributes.insert("shape_count".into(), shape_files.len().to_string());
        if !shape_files.is_empty() {
            attributes.insert("shapes".into(), shape_files.join(", "));
        }

        let thumbnail = entries.iter().find(|name| is_thumbnail_path(name)).cloned();
        attributes.insert(
            "thumbnail".into(),
            if thumbnail.is_some() { "yes" } else { "no" }.into(),
        );
        if let Some(thumb) = thumbnail {
            attributes.insert("thumbnail_file".into(), thumb);
        }

        // Tell the viewer how to turn this document into a mesh.
        if !shape_files.is_empty() {
            attributes.insert("tessellation".into(), "brep".into());
        }

        Ok(FileMetadata {
            format: "fcstd".into(),
            attributes,
        })
    }

    fn shape_payloads(&self, _path: &str, bytes: &[u8]) -> Result<Vec<ShapePayload>> {
        let mut archive = open_archive(bytes)?;
        let entries = entry_names(&mut archive)?;
        let available: BTreeSet<String> = entries
            .iter()
            .filter(|name| is_shape_path(name))
            .cloned()
            .collect();

        // Prefer the shapes that are actually part of the visible model. The
        // manifest distinguishes a PartDesign body's final shape from the
        // intermediate feature shapes it already contains; rendering both
        // would double up the geometry.
        let summary = match read_entry(&mut archive, DOCUMENT_XML)? {
            Some(xml) => parse_document(&xml)?,
            None => DocumentSummary::default(),
        };

        let mut selected: BTreeSet<String> = BTreeSet::new();
        for (object, file) in &summary.object_shape_files {
            if !available.contains(file) {
                continue;
            }
            let renderable = summary
                .object_types
                .get(object)
                .map(|ty| is_renderable_type(ty))
                .unwrap_or(true);
            if renderable {
                selected.insert(file.clone());
            }
        }
        // If the manifest gave us nothing usable, fall back to every shape in
        // the archive so the file is still viewable.
        if selected.is_empty() {
            selected = available;
        }

        let mut payloads = Vec::new();
        for name in selected {
            let data = read_entry(&mut archive, &name)?.unwrap_or_default();
            // Zero-length placeholders (e.g. suppressed shapes) carry nothing.
            if data.is_empty() {
                continue;
            }
            payloads.push(ShapePayload {
                name,
                format: "brep".into(),
                data,
            });
        }
        Ok(payloads)
    }
}

/// Document-level summary extracted from `Document.xml`.
#[derive(Debug, Default)]
struct DocumentSummary {
    schema_version: Option<String>,
    file_version: Option<String>,
    program_version: Option<String>,
    /// Document-level string properties, e.g. `Label` -> `"Block"`.
    document: BTreeMap<String, String>,
    /// Object name -> FreeCAD type, e.g. `Body` -> `PartDesign::Body`.
    object_types: BTreeMap<String, String>,
    /// Object name -> `.brp` file holding its shape.
    object_shape_files: Vec<(String, String)>,
}

/// Parse the parts of `Document.xml` we care about.
///
/// This is a streaming, namespace-agnostic scan rather than a DOM: we only
/// need the root attributes, the document-level properties and the
/// object/shape graph, and the manifest can be large.
fn parse_document(xml: &[u8]) -> Result<DocumentSummary> {
    let text = String::from_utf8_lossy(xml);
    let mut reader = Reader::from_str(&text);
    reader.config_mut().trim_text(true);

    let mut summary = DocumentSummary::default();
    let mut in_objects = false;
    let mut in_object_data = false;
    let mut current_document_property: Option<String> = None;
    let mut current_object: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| Error::parse("fcstd", format!("malformed Document.xml: {e}")))?;

        match event {
            Event::Eof => break,
            Event::Start(e) => match e.local_name().as_ref() {
                b"Document" => {
                    summary.schema_version = attribute(&e, b"SchemaVersion");
                    summary.file_version = attribute(&e, b"FileVersion");
                    summary.program_version = attribute(&e, b"ProgramVersion");
                }
                b"Objects" => in_objects = true,
                b"ObjectData" => in_object_data = true,
                b"Object" if in_object_data => {
                    current_object = attribute(&e, b"name");
                    if let (Some(name), Some(ty)) = (current_object.clone(), attribute(&e, b"type"))
                    {
                        summary.object_types.insert(name, ty);
                    }
                }
                b"Property" if !in_objects && !in_object_data => {
                    current_document_property = attribute(&e, b"name");
                }
                _ => {}
            },
            Event::Empty(e) => match e.local_name().as_ref() {
                b"Object" if in_objects && !in_object_data => {
                    if let (Some(name), Some(ty)) = (attribute(&e, b"name"), attribute(&e, b"type"))
                    {
                        summary.object_types.insert(name, ty);
                    }
                }
                b"Part" => {
                    if let (Some(object), Some(file)) =
                        (current_object.clone(), attribute(&e, b"file"))
                    {
                        summary.object_shape_files.push((object, file));
                    }
                }
                b"String" | b"Bool" | b"Integer" | b"Float" | b"UInt" => {
                    if let (Some(prop), Some(value)) =
                        (current_document_property.clone(), attribute(&e, b"value"))
                    {
                        summary.document.insert(prop, value);
                    }
                }
                _ => {}
            },
            Event::End(e) => match e.local_name().as_ref() {
                b"Objects" => in_objects = false,
                b"ObjectData" => in_object_data = false,
                b"Object" if in_object_data => current_object = None,
                b"Property" => current_document_property = None,
                _ => {}
            },
            _ => {}
        }
        buf.clear();
    }

    Ok(summary)
}

/// Is `ty` an object whose shape should be rendered directly?
///
/// PartDesign features below a body are already contained in the body's final
/// shape, sketches are wires rather than solids, and `App::*`/`Assembly::*`
/// objects are containers/datums. Everything else (e.g. `Part::Feature`,
/// `Part::Cut`, …) carries an independent shape.
fn is_renderable_type(ty: &str) -> bool {
    if ty.starts_with("Sketcher::") {
        return false;
    }
    if ty == "PartDesign::Body" {
        return true;
    }
    if ty.starts_with("PartDesign::") {
        return false;
    }
    if ty.starts_with("App::") || ty.starts_with("Assembly::") {
        return false;
    }
    true
}

fn summarise_object_types(summary: &DocumentSummary) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for ty in summary.object_types.values() {
        *counts.entry(ty.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(ty, n)| format!("{ty} ({n})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_shape_path(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".brp") || lower.ends_with(".brep")
}

fn is_thumbnail_path(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with("thumbnail.png")
}

fn open_archive(bytes: &[u8]) -> Result<ZipArchive<Cursor<&[u8]>>> {
    ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| Error::parse("fcstd", format!("not a valid FCStd/ZIP container: {e}")))
}

fn entry_names(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Vec<String>> {
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| Error::parse("fcstd", format!("corrupt ZIP directory: {e}")))?;
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
                .map_err(|e| Error::parse("fcstd", format!("reading {name}: {e}")))?;
            Ok(Some(out))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(Error::parse("fcstd", format!("reading {name}: {e}"))),
    }
}

/// Read an attribute by local name, tolerating any namespace prefix.
fn attribute(element: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|attr| attr.key.local_name().as_ref() == key)
        .and_then(|attr| attr.unescape_value().ok().map(|value| value.into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    const DOCUMENT_XML_FIXTURE: &str = r#"<?xml version='1.0' encoding='utf-8'?>
<Document SchemaVersion="4" ProgramVersion="1.0R" FileVersion="1">
    <Properties Count="3">
        <Property name="Label" type="App::PropertyString"><String value="Block"/></Property>
        <Property name="CreatedBy" type="App::PropertyString"><String value="Ada"/></Property>
        <Property name="CreationDate" type="App::PropertyString"><String value="2026-01-02T03:04:05+00:00"/></Property>
    </Properties>
    <Objects Count="3" Dependencies="1">
        <Object type="PartDesign::Body" name="Body" id="1" />
        <Object type="PartDesign::Pad" name="Pad" id="2" />
        <Object type="Sketcher::SketchObject" name="Sketch" id="3" />
    </Objects>
    <ObjectData Count="2">
        <Object name="Body">
            <Properties Count="1">
                <Property name="Shape" type="Part::PropertyPartShape"><Part file="Body.Shape.brp"/></Property>
            </Properties>
        </Object>
        <Object name="Pad">
            <Properties Count="1">
                <Property name="Shape" type="Part::PropertyPartShape"><Part file="Pad.Shape.brp"/></Property>
            </Properties>
        </Object>
    </ObjectData>
</Document>
"#;

    /// Build an in-memory `.FCStd` with the given entries.
    fn build_fcstd(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn block_document() -> Vec<u8> {
        build_fcstd(&[
            (DOCUMENT_XML, DOCUMENT_XML_FIXTURE.as_bytes()),
            ("Body.Shape.brp", b"CASCADE Topology V1\nLocations 0\n"),
            ("Pad.Shape.brp", b"CASCADE Topology V1\nLocations 0\n"),
            ("Sketch.Shape.brp", b""),
            ("thumbnails/Thumbnail.png", b"\x89PNG\r\n\x1a\n"),
        ])
    }

    #[test]
    fn extracts_document_metadata() {
        let bytes = block_document();
        let meta = FcstdHandler.metadata("Block.FCStd", &bytes).unwrap();

        assert_eq!(meta.format, "fcstd");
        assert_eq!(
            meta.attributes.get("label").map(String::as_str),
            Some("Block")
        );
        assert_eq!(
            meta.attributes.get("createdby").map(String::as_str),
            Some("Ada")
        );
        assert_eq!(
            meta.attributes.get("schema_version").map(String::as_str),
            Some("4")
        );
        assert_eq!(
            meta.attributes.get("object_count").map(String::as_str),
            Some("3")
        );
        assert_eq!(
            meta.attributes.get("thumbnail").map(String::as_str),
            Some("yes")
        );
        assert_eq!(
            meta.attributes.get("tessellation").map(String::as_str),
            Some("brep")
        );
        assert!(meta
            .attributes
            .get("object_types")
            .unwrap()
            .contains("PartDesign::Body (1)"));
    }

    #[test]
    fn selects_the_body_shape_not_its_features() {
        let bytes = block_document();
        let payloads = FcstdHandler.shape_payloads("Block.FCStd", &bytes).unwrap();
        let names: Vec<&str> = payloads.iter().map(|p| p.name.as_str()).collect();
        // The body's final shape is rendered; the Pad it contains is not.
        assert_eq!(names, vec!["Body.Shape.brp"]);
        assert_eq!(payloads[0].format, "brep");
        assert!(!payloads[0].data.is_empty());
    }

    #[test]
    fn falls_back_to_all_shapes_when_no_body_type_is_known() {
        // No ObjectData / type mapping: every shape in the archive is offered.
        let xml = "<Document><Objects Count=\"0\"></Objects></Document>";
        let bytes = build_fcstd(&[
            ("Document.xml", xml.as_bytes()),
            ("Shape.brp", b"brep-bytes"),
        ]);
        let payloads = FcstdHandler.shape_payloads("x.FCStd", &bytes).unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].name, "Shape.brp");
    }

    #[test]
    fn rejects_a_non_zip_container() {
        let err = FcstdHandler
            .metadata("nope.FCStd", b"this is not a zip")
            .unwrap_err();
        assert!(matches!(err, Error::Parse { .. }));
    }
}
