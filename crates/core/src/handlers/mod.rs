//! Pluggable CAD file handlers.
//!
//! The core is deliberately format-agnostic. Anything that needs to understand
//! a specific file type (extract metadata, tessellate to a mesh, later: extract
//! assemblies/BOM) is implemented behind [`CadHandler`] and registered in a
//! [`HandlerRegistry`]. Built-ins cover mesh formats (STL, OBJ, PLY, 3MF,
//! COLLADA), FreeCAD documents (`.FCStd`), exact-geometry interchange (STEP,
//! IGES, OpenCASCADE BREP), and a converter-backed handler for proprietary/native
//! CAD formats the core cannot parse itself.

pub mod brep;
pub mod converted;
pub mod dae;
pub mod fcstd;
pub mod generic;
pub mod iges;
pub mod obj;
pub mod ply;
pub mod step;
pub mod stl;
pub mod three_mf;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::convert::ConverterRegistry;
use crate::{Mesh, Result};

/// Format-agnostic metadata extracted from a file.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileMetadata {
    /// Handler/format name, e.g. `"stl"`.
    pub format: String,
    /// Free-form, sorted attributes for display. Keeping this a map makes it
    /// trivial to expose new properties without schema changes.
    pub attributes: BTreeMap<String, String>,
}

/// A raw shape payload that a WebGL client can tessellate out-of-process.
///
/// Some formats (notably FreeCAD `.FCStd`) store exact B-rep geometry rather
/// than a ready-made triangle mesh. The core cannot run a geometry kernel, so
/// instead of a [`Mesh`] it hands the client the original bytes and *how* to
/// interpret them; the desktop viewer feeds them to `occt-import-js`
/// (OpenCASCADE compiled to WebAssembly). See [`CadHandler::shape_payloads`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapePayload {
    /// Human-readable name, e.g. the source part within the container.
    pub name: String,
    /// Payload format the client should use to tessellate, e.g. `"brep"`.
    pub format: String,
    /// Raw payload bytes (e.g. the contents of a `.brp` file).
    pub data: Vec<u8>,
}

/// A handler for one or more file extensions.
pub trait CadHandler: Send + Sync {
    /// Stable identifier, e.g. `"stl"`.
    fn name(&self) -> &'static str;

    /// Extensions handled (lower-case, no dot). Empty slice means "fallback".
    fn extensions(&self) -> &'static [&'static str];

    /// Extract metadata. Handlers with richer knowledge override this.
    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        Ok(FileMetadata {
            format: self.name().to_string(),
            attributes: basic_attributes(path, bytes),
        })
    }

    /// Tessellate to a viewable mesh, if the format supports it.
    fn mesh(&self, _path: &str, _bytes: &[u8]) -> Result<Option<Mesh>> {
        Ok(None)
    }

    /// Raw shape payloads for client-side tessellation.
    ///
    /// Formats whose geometry is an exact B-rep (rather than triangles) return
    /// the payloads here; the desktop viewer passes each one to
    /// `occt-import-js` to build a mesh. Handlers that return payloads should
    /// advertise `tessellation = <format>` in their metadata so the UI knows to
    /// request them. Defaults to no payloads, i.e. "use [`Self::mesh`]".
    fn shape_payloads(&self, _path: &str, _bytes: &[u8]) -> Result<Vec<ShapePayload>> {
        Ok(Vec::new())
    }
}

/// Resolves a file path to the handler that should process it.
pub struct HandlerRegistry {
    handlers: Vec<Box<dyn CadHandler>>,
}

impl HandlerRegistry {
    /// The default set of handlers: STL, OBJ, PLY, 3MF, COLLADA, IGES, BREP,
    /// FreeCAD, STEP, the converter-backed handler (metadata only until a
    /// converter is configured) and a catch-all fallback.
    pub fn builtin() -> Self {
        Self::with_converters(Arc::new(ConverterRegistry::empty()))
    }

    /// Like [`Self::builtin`], but with `converters` backing the
    /// [`converted::ConvertibleHandler`] so proprietary formats can be
    /// previewed through an external engine.
    pub fn with_converters(converters: Arc<ConverterRegistry>) -> Self {
        Self {
            handlers: vec![
                Box::new(stl::StlHandler),
                Box::new(obj::ObjHandler),
                Box::new(ply::PlyHandler),
                Box::new(three_mf::ThreeMfHandler),
                Box::new(dae::DaeHandler),
                Box::new(iges::IgesHandler),
                Box::new(brep::BrepHandler),
                Box::new(fcstd::FcstdHandler),
                Box::new(step::StepHandler),
                Box::new(converted::ConvertibleHandler::new(converters)),
                Box::new(generic::GenericHandler),
            ],
        }
    }

    /// Registry for the app, loading converters from the per-user
    /// `$HOME/.openpdm/converters.json` (see [`crate::convert`] and
    /// [`crate::app`]). Converters are app-level, so the same backend is used
    /// for every project.
    pub fn for_app() -> Self {
        Self::with_converters(Arc::new(ConverterRegistry::load(&crate::app::dir())))
    }

    /// Find the handler for `path`, falling back to the generic handler.
    pub fn resolve(&self, path: &str) -> &dyn CadHandler {
        let ext = extension_of(path);
        self.handlers
            .iter()
            .find(|h| !h.extensions().is_empty() && h.extensions().contains(&ext.as_str()))
            .or_else(|| self.handlers.iter().find(|h| h.extensions().is_empty()))
            .map(|h| h.as_ref())
            .expect("builtin registry must contain a fallback handler")
    }

    /// `(name, extensions)` for every registered handler, for introspection.
    pub fn list(&self) -> Vec<(&'static str, &'static [&'static str])> {
        self.handlers
            .iter()
            .map(|h| (h.name(), h.extensions()))
            .collect()
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

/// File extensions OpenPDM treats as project content.
///
/// The app is CAD-focused: only files with one of these extensions are listed
/// in the UI and staged into a revision. Everything else in the working tree
/// (repo metadata, notes, generated files like the `*.pdmdb` index) is ignored.
///
/// This is deliberately broader than the set of registered [`CadHandler`]s —
/// formats such as IGES and the native CAD formats are tracked as project
/// content even before the core learns to preview them. The
/// `allowed_extensions_cover_handlers` test keeps the two lists from drifting.
pub const PROJECT_EXTENSIONS: &[&str] = &[
    // Meshes and interchange formats.
    "stl", "obj", "ply", "3mf", "gltf", "glb", "dae",
    // FreeCAD.
    "fcstd",
    // STEP / IGES.
    "step", "stp", "iges", "igs",
    // Native CAD: SolidWorks, Inventor, Creo/NX, CATIA, Fusion, Rhino.
    "sldprt", "sldasm", "ipt", "iam", "prt", "asm", "catpart", "catproduct", "f3d", "3dm",
    // Kernel / exchange geometry.
    "sat", "sab", "x_t", "x_b", "brep", "brp",
    // 2D drawings.
    "dxf", "dwg",
];

/// Whether `path` names a CAD application lock/temp file.
///
/// SolidWorks (and other Windows apps) drop a transitory `~$` file beside a
/// document while it is open — e.g. `~$Block.SLDPRT`. These are not project
/// content: they appear and vanish with the application, so they are never
/// listed or staged even though their extension otherwise looks like a CAD
/// format.
pub fn is_lock_file(path: &str) -> bool {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("~$"))
}

/// Whether `path` has an extension that counts as project content.
///
/// CAD lock files (see [`is_lock_file`]) are excluded even when their extension
/// is a known CAD format.
pub fn is_project_file(path: &str) -> bool {
    let ext = extension_of(path);
    !is_lock_file(path) && !ext.is_empty() && PROJECT_EXTENSIONS.contains(&ext.as_str())
}

/// Attributes every handler reports: extension and byte size.
pub(crate) fn basic_attributes(path: &str, bytes: &[u8]) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    let ext = extension_of(path);
    attrs.insert(
        "extension".to_string(),
        if ext.is_empty() {
            "(none)".to_string()
        } else {
            ext
        },
    );
    attrs.insert("size_bytes".to_string(), bytes.len().to_string());
    attrs
}

pub(crate) fn extension_of(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) fn fmt_vec3(v: [f32; 3]) -> String {
    format!("{:.3}, {:.3}, {:.3}", v[0], v[1], v[2])
}

/// The file name of `path` (for naming a shape payload), falling back to the
/// path itself.
pub(crate) fn payload_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_by_extension_and_falls_back() {
        let reg = HandlerRegistry::builtin();
        assert_eq!(reg.resolve("a.stl").name(), "stl");
        assert_eq!(reg.resolve("a.STL").name(), "stl");
        assert_eq!(reg.resolve("a.obj").name(), "obj");
        assert_eq!(reg.resolve("a.ply").name(), "ply");
        assert_eq!(reg.resolve("a.PLY").name(), "ply");
        assert_eq!(reg.resolve("a.3mf").name(), "3mf");
        assert_eq!(reg.resolve("a.3MF").name(), "3mf");
        assert_eq!(reg.resolve("a.dae").name(), "dae");
        assert_eq!(reg.resolve("a.DAE").name(), "dae");
        assert_eq!(reg.resolve("a.fcstd").name(), "fcstd");
        assert_eq!(reg.resolve("a.FCStd").name(), "fcstd");
        assert_eq!(reg.resolve("a.sldprt").name(), "converted");
        assert_eq!(reg.resolve("a.SLDPRT").name(), "converted");
        assert_eq!(reg.resolve("a.sldasm").name(), "converted");
        assert_eq!(reg.resolve("a.catpart").name(), "converted");
        assert_eq!(reg.resolve("a.step").name(), "step");
        assert_eq!(reg.resolve("a.STP").name(), "step");
        assert_eq!(reg.resolve("a.iges").name(), "iges");
        assert_eq!(reg.resolve("a.IGS").name(), "iges");
        assert_eq!(reg.resolve("a.brep").name(), "brep");
        assert_eq!(reg.resolve("a.brp").name(), "brep");
        assert_eq!(reg.resolve("noext").name(), "generic");
    }

    #[test]
    fn identifies_project_files() {
        assert!(is_project_file("part.stl"));
        assert!(is_project_file("assembly.SLDPRT"));
        assert!(is_project_file("drawing.DXF"));
        assert!(is_project_file("scene.dae"));
        assert!(is_project_file("bracket.step"));
        assert!(!is_project_file("index.pdmdb"));
        assert!(!is_project_file("README.md"));
        assert!(!is_project_file(".gitignore"));
        assert!(!is_project_file("noext"));
        // CAD application lock files are never project content, even though the
        // extension alone would otherwise qualify.
        assert!(is_lock_file("~$Block.SLDPRT"));
        assert!(is_lock_file("parts/~$Block.SLDPRT"));
        assert!(is_lock_file("~$Assembly.SLDASM"));
        assert!(!is_lock_file("Block.SLDPRT"));
        assert!(!is_project_file("~$Block.SLDPRT"));
        assert!(!is_project_file("parts/~$Block.SLDPRT"));
    }

    #[test]
    fn allowed_extensions_cover_handlers() {
        let reg = HandlerRegistry::builtin();
        for (name, extensions) in reg.list() {
            for ext in extensions {
                assert!(
                    is_project_file(&format!("file.{ext}")),
                    "handler {name} extension {ext} missing from PROJECT_EXTENSIONS"
                );
            }
        }
    }
}
