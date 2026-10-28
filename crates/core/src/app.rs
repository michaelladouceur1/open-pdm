//! Per-user application configuration directory.
//!
//! OpenPDM keeps its app-level configuration in `$HOME/.openpdm` on every OS:
//! the converter registry ([`converters_file`]) and the [`bin_dir`] directory
//! where converter executables are installed.
//!
//! This is deliberately separate from the per-project `.openpdm/` directory
//! (see [`crate::git::INTERNAL_DIR`]). A converter is an *app*-level tool —
//! configure or install it once and it works in every project — whereas the
//! SQLite index describes one repository and stays beside it.

use std::path::{Path, PathBuf};

use crate::Result;

/// Name of the per-user application directory.
///
/// It mirrors the per-project [`INTERNAL_DIR`](crate::git::INTERNAL_DIR) name
/// but is rooted at the user's home directory rather than a repository.
pub const APP_DIR: &str = ".openpdm";

/// The per-user application directory, `$HOME/.openpdm`.
pub fn dir() -> PathBuf {
    home_dir().join(APP_DIR)
}

/// Directory converter executables are installed into, `$HOME/.openpdm/bin`.
///
/// A converter whose `program` is a bare name is looked up here (before the
/// process `PATH`), so an installed converter works in every project.
pub fn bin_dir() -> PathBuf {
    dir().join("bin")
}

/// The converter registry file, `$HOME/.openpdm/converters.json`.
pub fn converters_file() -> PathBuf {
    dir().join("converters.json")
}

/// The app-level tracking settings file, `$HOME/.openpdm/settings.json`.
///
/// Like converters, these settings are shared across every project.
pub fn settings_file() -> PathBuf {
    dir().join("settings.json")
}

/// Root of the converted-artifact cache, `$HOME/.openpdm/cache`.
pub fn cache_dir() -> PathBuf {
    dir().join("cache")
}

/// Total size, in bytes, of everything under the app [`cache_dir`].
///
/// Returns 0 when the cache does not exist yet. Used by the settings dialog to
/// report how much disk the converted-artifact cache is using.
pub fn cache_size() -> u64 {
    dir_size(&cache_dir())
}

/// Remove everything under the app [`cache_dir`], keeping the directory itself.
///
/// The cache is a rebuildable convenience (converted previews are regenerated
/// on demand), so clearing it is always safe. Missing directory is a no-op.
pub fn clear_cache() -> Result<()> {
    clear_dir(&cache_dir())
}

/// Recursively sum the sizes of the files under `path` (0 if it is missing).
fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Remove the contents of `path` without removing `path` itself.
fn clear_dir(path: &Path) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let target = entry.path();
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            std::fs::remove_dir_all(&target)?;
        } else {
            std::fs::remove_file(&target)?;
        }
    }
    Ok(())
}

/// Create the application directory, its `bin` subdirectory and an empty
/// `converters.json`, skipping anything that already exists.
///
/// Called when the app starts so the folder the user edits is discoverable
/// without having to run a converter first. Idempotent, and an existing
/// `converters.json` is never overwritten.
pub fn ensure() -> Result<()> {
    ensure_at(&dir())
}

/// [`ensure`] for an explicit directory. Useful for tests and for callers that
/// resolve the location themselves.
pub fn ensure_at(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir.join("bin"))?;
    let config = dir.join("converters.json");
    if !config.exists() {
        std::fs::write(&config, "{\n  \"converters\": {}\n}\n")?;
    }
    Ok(())
}

/// The current user's home directory.
///
/// On Unix `HOME` is canonical. On Windows the profile is taken from
/// `USERPROFILE` first, because `HOME` may hold a POSIX-style path (for example
/// under Git Bash) that does not name a native directory; `HOME`,
/// `HOMEDRIVE`+`HOMEPATH` are used as fallbacks.
fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(profile) = non_empty_var("USERPROFILE") {
            return profile;
        }
        if let Some(home) = non_empty_var("HOME") {
            return home;
        }
        let drive = std::env::var_os("HOMEDRIVE");
        let path = std::env::var_os("HOMEPATH");
        if let (Some(drive), Some(path)) = (drive, path) {
            let mut home = PathBuf::from(drive);
            home.push(path);
            return home;
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = non_empty_var("HOME") {
            return home;
        }
    }
    // Last resort: the process working directory. Better a usable relative
    // `.openpdm` than no configuration at all.
    PathBuf::from(".")
}

fn non_empty_var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_paths_live_under_the_app_dir() {
        let dir = dir();
        assert_eq!(dir.join("bin"), bin_dir());
        assert_eq!(dir.join("converters.json"), converters_file());
        assert_eq!(dir.join("cache"), cache_dir());
        assert!(dir.ends_with(APP_DIR));
    }

    #[test]
    fn ensure_creates_directory_bin_and_config() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("app");
        ensure_at(&root).unwrap();

        assert!(root.join("bin").is_dir());
        assert!(root.join("converters.json").is_file());
        assert!(std::fs::read_to_string(root.join("converters.json"))
            .unwrap()
            .contains("converters"));
    }

    #[test]
    fn ensure_does_not_overwrite_existing_config() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path()).unwrap();
        let config = tmp.path().join("converters.json");
        std::fs::write(
            &config,
            r#"{ "converters": { "prt": { "program": "creo" } } }"#,
        )
        .unwrap();

        ensure_at(tmp.path()).unwrap();

        let text = std::fs::read_to_string(&config).unwrap();
        assert!(text.contains("creo"));
    }

    #[test]
    fn cache_size_sums_and_clear_removes_contents() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path();
        std::fs::create_dir_all(cache.join("stl").join("conv")).unwrap();
        std::fs::write(cache.join("stl").join("conv").join("a.stl"), vec![0u8; 10]).unwrap();
        std::fs::write(cache.join("stl").join("b.stl"), vec![0u8; 5]).unwrap();

        assert_eq!(dir_size(cache), 15);
        clear_dir(cache).unwrap();
        assert_eq!(dir_size(cache), 0);
        // The cache directory itself is kept.
        assert!(cache.is_dir());
    }
}
