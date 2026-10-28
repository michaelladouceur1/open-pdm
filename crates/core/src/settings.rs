//! User-configurable file-tracking settings.
//!
//! OpenPDM recognises a fixed set of CAD file types as project content (see
//! [`PROJECT_EXTENSIONS`](crate::handlers::PROJECT_EXTENSIONS)). Those settings
//! let a user *narrow* that set: a file type can be excluded from tracking
//! either app-wide (`$HOME/.openpdm/settings.json`) or for a single project
//! (`<project>/.openpdm/settings.json`).
//!
//! Ignored types still exist on disk and in git; they are simply not listed as
//! project content and not staged or indexed. The two scopes combine: an
//! extension ignored at either level is ignored overall.
//!
//! Settings store an *ignore list* rather than an allow list, so a newly
//! supported CAD format is tracked by default without a migration.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::handlers::extension_of;
use crate::Result;

/// File types (by extension) the user has opted out of tracking.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// Lower-case extensions (without the dot) that are not tracked.
    #[serde(default)]
    pub ignored_extensions: Vec<String>,
}

impl Settings {
    /// Read settings from `path`. A missing, unreadable or malformed file
    /// yields the defaults (track everything), so a corrupt file never breaks
    /// the app.
    pub fn load(path: impl AsRef<Path>) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Write settings to `path`, creating the parent directory if needed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let text = serde_json::to_string_pretty(self)
            .unwrap_or_else(|_| "{\n  \"ignored_extensions\": []\n}".to_string());
        std::fs::write(path, format!("{text}\n"))?;
        Ok(())
    }

    /// Whether `path`'s extension is in the ignore list.
    pub fn ignores(&self, path: &str) -> bool {
        let ext = extension_of(path);
        !ext.is_empty()
            && self
                .ignored_extensions
                .iter()
                .any(|ignored| ignored.eq_ignore_ascii_case(&ext))
    }

    /// Whether `path` is project content in the ordinary sense *and* not
    /// ignored by `self`.
    pub fn tracks(&self, path: &str) -> bool {
        crate::handlers::is_project_file(path) && !self.ignores(path)
    }
}

/// Whether `path` should be tracked given the app- and project-level settings.
///
/// A file is tracked when it is recognised project content and is not ignored
/// at either scope.
pub fn is_tracked(path: &str, app: &Settings, project: &Settings) -> bool {
    app.tracks(path) && !project.ignores(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_track_everything() {
        let settings = Settings::default();
        assert!(settings.tracks("part.stl"));
        assert!(settings.tracks("bracket.step"));
        // Non-project content is still never tracked.
        assert!(!settings.tracks("README.md"));
    }

    #[test]
    fn ignored_extension_is_not_tracked() {
        let settings = Settings {
            ignored_extensions: vec!["glb".to_string()],
        };
        assert!(settings.ignores("scene.GLB"));
        assert!(!settings.tracks("scene.glb"));
        assert!(settings.tracks("scene.gltf"));
    }

    #[test]
    fn app_and_project_scopes_combine() {
        let app = Settings {
            ignored_extensions: vec!["glb".to_string()],
        };
        let project = Settings {
            ignored_extensions: vec!["step".to_string()],
        };
        assert!(!is_tracked("scene.glb", &app, &project));
        assert!(!is_tracked("bracket.step", &app, &project));
        assert!(is_tracked("part.stl", &app, &project));
    }

    #[test]
    fn save_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("settings.json");
        let settings = Settings {
            ignored_extensions: vec!["glb".to_string(), "dae".to_string()],
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        // A missing file falls back to defaults.
        assert_eq!(Settings::load(tmp.path().join("absent.json")), Settings::default());
    }
}
