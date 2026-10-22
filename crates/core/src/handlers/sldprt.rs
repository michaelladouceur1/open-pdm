//! SolidWorks (`.sldprt` / `.sldasm`) handler.
//!
//! SolidWorks files are proprietary: there is no in-process parser, so the
//! exact geometry can only be read by an external engine the user configures
//! (SolidWorks, eDrawings, a commercial SDK, …). This handler delegates to a
//! [`Converter`] to produce an intermediate STL, then reuses [`StlHandler`] to
//! turn that into a [`Mesh`] — so the desktop viewer needs no SolidWorks
//! knowledge at all.
//!
//! Without a configured converter the file still lists and reports metadata
//! (the metadata advertises `preview = unavailable` so the UI can explain the
//! situation); only the 3D preview is missing.

use std::sync::Arc;

use crate::convert::{Converter, TARGET_STL};
use crate::{Mesh, Result};

use super::stl::StlHandler;
use super::{basic_attributes, extension_of, CadHandler, FileMetadata};

/// SolidWorks parts and assemblies, previewed via an external converter.
pub struct SldprtHandler {
    converter: Arc<dyn Converter>,
}

impl SldprtHandler {
    pub fn new(converter: Arc<dyn Converter>) -> Self {
        Self { converter }
    }
}

impl CadHandler for SldprtHandler {
    fn name(&self) -> &'static str {
        "sldprt"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["sldprt", "sldasm"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let mut attributes = basic_attributes(path, bytes);
        attributes.insert("proprietary".into(), "yes".into());
        if self.converter.available() {
            attributes.insert("preview".into(), "converter".into());
            attributes.insert("converter".into(), self.converter.id());
        } else {
            attributes.insert("preview".into(), "unavailable".into());
        }
        let ext = extension_of(path);
        Ok(FileMetadata {
            format: if ext.is_empty() { self.name().into() } else { ext },
            attributes,
        })
    }

    fn mesh(&self, path: &str, bytes: &[u8]) -> Result<Option<Mesh>> {
        if !self.converter.available() {
            return Ok(None);
        }
        let stl = self.converter.convert(TARGET_STL, path, bytes)?;
        StlHandler.mesh("preview.stl", &stl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::NullConverter;

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

    #[test]
    fn metadata_marks_preview_unavailable_without_converter() {
        let handler = SldprtHandler::new(Arc::new(NullConverter));
        let meta = handler.metadata("part.sldprt", b"x").unwrap();
        assert_eq!(meta.format, "sldprt");
        assert_eq!(meta.attributes.get("preview").unwrap(), "unavailable");
        assert!(handler.mesh("part.sldprt", b"x").unwrap().is_none());
    }

    #[test]
    fn metadata_reports_converter_when_configured() {
        let handler = SldprtHandler::new(Arc::new(FakeConverter));
        let meta = handler.metadata("assembly.sldasm", b"x").unwrap();
        assert_eq!(meta.format, "sldasm");
        assert_eq!(meta.attributes.get("preview").unwrap(), "converter");
        assert_eq!(meta.attributes.get("converter").unwrap(), "fake");
    }

    #[test]
    fn mesh_parses_converted_stl() {
        let handler = SldprtHandler::new(Arc::new(FakeConverter));
        let mesh = handler.mesh("part.sldprt", b"x").unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.vertex_count(), 3);
    }
}
