//! OpenCASCADE BREP (`.brep` / `.brp`) handler.
//!
//! BREP is OpenCASCADE's native exact boundary-representation format: an ASCII
//! listing of the shape's locations, curves, surfaces and topology. It is not a
//! triangle mesh and the core has no geometry kernel, so this handler reads the
//! section headers for metadata and advertises `tessellation = brep`; the
//! desktop viewer then hands the raw bytes to `occt-import-js`, which reads it
//! with `ReadBrepFile` and tessellates it in the webview. The file _is_ the
//! payload, so no external converter is needed.
//!
//! The same format is also used for the per-shape `.brp` payloads embedded in a
//! FreeCAD `.FCStd` container (see [`super::fcstd`]); this handler covers
//! standalone `.brep`/`.brp` files.

use std::collections::BTreeMap;

use crate::Result;

use super::{basic_attributes, CadHandler, FileMetadata, ShapePayload};

/// OpenCASCADE BREP handler: `.brep` / `.brp`.
pub struct BrepHandler;

impl CadHandler for BrepHandler {
    fn name(&self) -> &'static str {
        "brep"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["brep", "brp"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let summary = parse(bytes);
        let mut attributes = basic_attributes(path, bytes);
        if !summary.version.is_empty() {
            attributes.insert("version".into(), summary.version.clone());
        }
        insert_count(&mut attributes, "locations", summary.locations);
        insert_count(&mut attributes, "curve2ds", summary.curve2ds);
        insert_count(&mut attributes, "curves", summary.curves);
        insert_count(&mut attributes, "polygon3d", summary.polygon3d);
        insert_count(&mut attributes, "surfaces", summary.surfaces);
        insert_count(&mut attributes, "triangulations", summary.triangulations);
        insert_count(&mut attributes, "tshapes", summary.tshapes);
        // Tell the viewer to request the payload and tessellate it as B-rep.
        attributes.insert("tessellation".into(), "brep".into());
        Ok(FileMetadata {
            format: "brep".into(),
            attributes,
        })
    }

    fn shape_payloads(&self, path: &str, bytes: &[u8]) -> Result<Vec<ShapePayload>> {
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![ShapePayload {
            name: super::payload_name(path),
            format: "brep".into(),
            data: bytes.to_vec(),
        }])
    }
}

/// Section counts read from the BREP listing.
#[derive(Default)]
struct BrepSummary {
    version: String,
    locations: Option<usize>,
    curve2ds: Option<usize>,
    curves: Option<usize>,
    polygon3d: Option<usize>,
    surfaces: Option<usize>,
    triangulations: Option<usize>,
    tshapes: Option<usize>,
}

/// Read the version banner and per-section counts from the ASCII listing.
///
/// Each section is introduced by a `Keyword <count>` line; only those lines are
/// matched, so the (large) geometry bodies are walked but not interpreted.
fn parse(bytes: &[u8]) -> BrepSummary {
    let text = String::from_utf8_lossy(bytes);
    let mut summary = BrepSummary::default();

    for (position, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if position == 0 {
            summary.version = line.to_string();
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(keyword), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Ok(count) = value.parse::<usize>() else {
            continue;
        };
        match keyword {
            "Locations" => summary.locations = Some(count),
            "Curve2ds" => summary.curve2ds = Some(count),
            "Curves" => summary.curves = Some(count),
            "Polygon3D" => summary.polygon3d = Some(count),
            "Surfaces" => summary.surfaces = Some(count),
            "Triangulations" => summary.triangulations = Some(count),
            "TShapes" => summary.tshapes = Some(count),
            _ => {}
        }
    }

    summary
}

fn insert_count(attributes: &mut BTreeMap<String, String>, key: &str, count: Option<usize>) {
    if let Some(count) = count.filter(|count| *count > 0) {
        attributes.insert(key.to_string(), count.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BREP: &str = "CASCADE Topology V1, (c) Matra-Datavision\n\
Locations 1\n\
Curve2ds 0\n\
Curves 2\n\
Polygon3D 0\n\
PolygonOnTriangulations 0\n\
Surfaces 3\n\
Triangulations 0\n\
TShapes 4\n";

    #[test]
    fn parses_section_counts() {
        let meta = BrepHandler.metadata("part.brep", BREP.as_bytes()).unwrap();
        assert_eq!(meta.format, "brep");
        assert_eq!(
            meta.attributes.get("version").map(String::as_str),
            Some("CASCADE Topology V1, (c) Matra-Datavision")
        );
        assert_eq!(meta.attributes.get("locations").map(String::as_str), Some("1"));
        assert_eq!(meta.attributes.get("curves").map(String::as_str), Some("2"));
        assert_eq!(meta.attributes.get("surfaces").map(String::as_str), Some("3"));
        assert_eq!(meta.attributes.get("tshapes").map(String::as_str), Some("4"));
        // Zero-count sections are omitted to keep the metadata tidy.
        assert!(!meta.attributes.contains_key("curve2ds"));
        assert_eq!(
            meta.attributes.get("tessellation").map(String::as_str),
            Some("brep")
        );
    }

    #[test]
    fn exposes_the_file_as_a_brep_payload() {
        let payloads = BrepHandler.shape_payloads("parts/part.brp", BREP.as_bytes()).unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].name, "part.brp");
        assert_eq!(payloads[0].format, "brep");
        assert_eq!(payloads[0].data, BREP.as_bytes());
    }

    #[test]
    fn empty_file_has_no_payload() {
        assert!(BrepHandler.shape_payloads("empty.brep", b"").unwrap().is_empty());
    }

    #[test]
    fn tolerates_a_file_without_section_headers() {
        let meta = BrepHandler.metadata("broken.brep", b"not a real brep\n").unwrap();
        assert_eq!(meta.format, "brep");
        assert_eq!(
            meta.attributes.get("tessellation").map(String::as_str),
            Some("brep")
        );
    }
}
