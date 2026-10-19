use crate::Result;

use super::CadHandler;

/// Catch-all handler for formats the core does not specifically understand.
/// Keeping it registered guarantees every file can at least be listed and
/// described; format-specific behaviour becomes a later handler/sidecar.
pub struct GenericHandler;

impl CadHandler for GenericHandler {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[]
    }

    fn mesh(&self, _path: &str, _bytes: &[u8]) -> Result<Option<crate::Mesh>> {
        Ok(None)
    }
}
