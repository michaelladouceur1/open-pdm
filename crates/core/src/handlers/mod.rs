//! Pluggable CAD file handlers.
//!
//! The core is deliberately format-agnostic. Anything that needs to understand
//! a specific file type (extract metadata, tessellate to a mesh, later: extract
//! assemblies/BOM) is implemented behind [`CadHandler`] and registered in a
//! [`HandlerRegistry`]. Built-ins cover mesh formats (STL, OBJ) and FreeCAD
//! documents (`.FCStd`); a future sidecar/JSON-RPC mechanism can add
//! proprietary formats (e.g. SolidWorks) without touching the core.

pub mod fcstd;
pub mod generic;
pub mod obj;
pub mod stl;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
    /// The default set of handlers: STL, OBJ, FreeCAD, and a catch-all fallback.
    pub fn builtin() -> Self {
        Self {
            handlers: vec![
                Box::new(stl::StlHandler),
                Box::new(obj::ObjHandler),
                Box::new(fcstd::FcstdHandler),
                Box::new(generic::GenericHandler),
            ],
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_by_extension_and_falls_back() {
        let reg = HandlerRegistry::builtin();
        assert_eq!(reg.resolve("a.stl").name(), "stl");
        assert_eq!(reg.resolve("a.STL").name(), "stl");
        assert_eq!(reg.resolve("a.obj").name(), "obj");
        assert_eq!(reg.resolve("a.fcstd").name(), "fcstd");
        assert_eq!(reg.resolve("a.FCStd").name(), "fcstd");
        assert_eq!(reg.resolve("a.sldprt").name(), "generic");
        assert_eq!(reg.resolve("noext").name(), "generic");
    }
}
