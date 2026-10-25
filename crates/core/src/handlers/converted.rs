//! Converter-backed handler for formats the core cannot parse itself.
//!
//! Native/proprietary CAD formats (SolidWorks, Inventor, Creo/NX, CATIA, …) and
//! kernel/exchange geometry (Parasolid, ACIS, IGES, …) have no in-process parser
//! here. Rather than special-case each one, this single handler claims their
//! extensions and, when the user has configured an external [`Converter`] for
//! that format (see [`crate::convert`]), converts to an intermediate STL and
//! reuses [`StlHandler`] to build the [`Mesh`]. The desktop viewer therefore
//! needs no format-specific knowledge at all.
//!
//! Without a configured backend the file still lists and reports metadata (the
//! metadata advertises `preview = unavailable` so the UI can explain the
//! situation); only the 3D preview is missing.

use std::sync::Arc;

use crate::convert::{ConverterRegistry, TARGET_STL};
use crate::{Mesh, Result};

use super::stl::StlHandler;
use super::{basic_attributes, extension_of, CadHandler, FileMetadata};

/// Extensions this handler can preview when a converter is configured for them.
///
/// These are the allowlisted formats (see [`super::PROJECT_EXTENSIONS`]) the core
/// has no native parser for. Mesh formats (STL/OBJ/PLY/3MF), FreeCAD and 2D
/// drawings are handled elsewhere and deliberately excluded.
pub const CONVERTIBLE_EXTENSIONS: &[&str] = &[
    // SolidWorks.
    "sldprt", "sldasm",
    // Autodesk Inventor.
    "ipt", "iam",
    // Creo / Pro-E (also NX / Solid Edge, which share these extensions).
    "prt", "asm",
    // CATIA.
    "catpart", "catproduct",
    // Fusion 360 / Rhino.
    "f3d", "3dm",
    // Kernel / exchange geometry.
    "sat", "sab", "x_t", "x_b", "brep", "brp",
    // IGES (STEP is handled natively by `StepHandler`).
    "iges", "igs",
];

/// Routes proprietary/native CAD formats through their configured converter.
pub struct ConvertibleHandler {
    converters: Arc<ConverterRegistry>,
}

impl ConvertibleHandler {
    pub fn new(converters: Arc<ConverterRegistry>) -> Self {
        Self { converters }
    }

    /// The converter for `path`'s format, if one is configured and runnable.
    fn usable_converter(&self, path: &str) -> Option<Arc<dyn crate::convert::Converter>> {
        self.converters
            .converter_for(&extension_of(path))
            .filter(|converter| converter.available())
    }
}

impl CadHandler for ConvertibleHandler {
    fn name(&self) -> &'static str {
        "converted"
    }

    fn extensions(&self) -> &'static [&'static str] {
        CONVERTIBLE_EXTENSIONS
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let mut attributes = basic_attributes(path, bytes);
        match self.usable_converter(path) {
            Some(converter) => {
                attributes.insert("preview".into(), "converter".into());
                attributes.insert("converter".into(), converter.id());
            }
            None => {
                attributes.insert("preview".into(), "unavailable".into());
            }
        }
        let ext = extension_of(path);
        Ok(FileMetadata {
            format: if ext.is_empty() { self.name().into() } else { ext },
            attributes,
        })
    }

    fn mesh(&self, path: &str, bytes: &[u8]) -> Result<Option<Mesh>> {
        match self.usable_converter(path) {
            Some(converter) => {
                let stl = converter.convert(TARGET_STL, path, bytes)?;
                StlHandler.mesh("preview.stl", &stl)
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::Converter;

    const ASCII_STL: &[u8] = b"solid s\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\n\
         vertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid s\n";

    /// Converter that hands back a fixed STL, standing in for a real engine.
    struct FakeConverter;

    impl Converter for FakeConverter {
        fn id(&self) -> String {
            "fake".to_string()
        }
        fn available(&self) -> bool {
            true
        }
        fn convert(&self, _target: &str, _input_name: &str, _bytes: &[u8]) -> Result<Vec<u8>> {
            Ok(ASCII_STL.to_vec())
        }
    }

    fn registry_with(extension: &str) -> Arc<ConverterRegistry> {
        Arc::new(ConverterRegistry::from_backends([(
            extension.to_string(),
            Arc::new(FakeConverter) as Arc<dyn Converter>,
        )]))
    }

    #[test]
    fn metadata_marks_preview_unavailable_without_backend() {
        let handler = ConvertibleHandler::new(Arc::new(ConverterRegistry::empty()));
        let meta = handler.metadata("part.sldprt", b"x").unwrap();
        assert_eq!(meta.format, "sldprt");
        assert_eq!(meta.attributes.get("preview").unwrap(), "unavailable");
        assert!(handler.mesh("part.sldprt", b"x").unwrap().is_none());
    }

    #[test]
    fn metadata_reports_backend_for_configured_format() {
        let handler = ConvertibleHandler::new(registry_with("sldasm"));
        let meta = handler.metadata("assembly.sldasm", b"x").unwrap();
        // The extension drives the reported format, not the handler name.
        assert_eq!(meta.format, "sldasm");
        assert_eq!(meta.attributes.get("preview").unwrap(), "converter");
        assert_eq!(meta.attributes.get("converter").unwrap(), "fake");
    }

    #[test]
    fn mesh_parses_converted_stl_for_configured_format() {
        let handler = ConvertibleHandler::new(registry_with("sldprt"));
        let mesh = handler.mesh("part.sldprt", b"x").unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.vertex_count(), 3);
    }

    #[test]
    fn unconfigured_extension_has_no_mesh() {
        let handler = ConvertibleHandler::new(registry_with("sldprt"));
        // Handled by this handler, but no backend is configured for it.
        assert!(handler.mesh("part.catpart", b"x").unwrap().is_none());
    }
}
