//! External conversion for formats the core cannot parse itself.
//!
//! Some CAD formats — notably SolidWorks `.sldprt`/`.sldasm` — are proprietary:
//! there is no in-process parser, and the exact geometry can only be read by an
//! engine the user supplies (SolidWorks, eDrawings, a commercial SDK, …). This
//! module is the seam for that: a [`Converter`] turns input bytes into an
//! intermediate format the core *can* parse (currently [`TARGET_STL`]), and
//! [`CommandConverter`] implements it by running a user-configured program.
//!
//! Converted output is cached under `<root>/.openpdm/cache/<target>/`, keyed by
//! the input's git blob oid and the converter's [`Converter::id`]. That keeps
//! the core content-addressed (identical geometry converts once, across
//! revisions) and invalidates automatically when the converter changes. Like
//! the SQLite index, the cache is machine-local and never committed.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use git2::{ObjectType, Oid};
use serde::Deserialize;

use crate::{Error, Result};

/// Intermediate format used for converted previews.
pub const TARGET_STL: &str = "stl";

/// How long a converter process may run before it is killed.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Turns one file's bytes into `target`-format bytes using an external engine.
pub trait Converter: Send + Sync {
    /// Stable identifier, e.g. `"command:my-converter"`. Part of the cache key,
    /// so changing it invalidates previously cached conversions.
    fn id(&self) -> String;

    /// Whether this converter can actually run. When `false` callers should
    /// degrade gracefully (metadata only) instead of erroring.
    fn available(&self) -> bool;

    /// Convert `bytes` to `target`. `input_name` is the original (repo-relative)
    /// file name; its extension is preserved on the scratch input file so
    /// extension-sniffing tools behave.
    fn convert(&self, target: &str, input_name: &str, bytes: &[u8]) -> Result<Vec<u8>>;
}

/// Default converter: unavailable. Used whenever no external engine is
/// configured, so proprietary files fall back to metadata-only.
pub struct NullConverter;

impl Converter for NullConverter {
    fn id(&self) -> String {
        "none".to_string()
    }

    fn available(&self) -> bool {
        false
    }

    fn convert(&self, _target: &str, _input_name: &str, _bytes: &[u8]) -> Result<Vec<u8>> {
        Err(Error::Conversion("no converter configured".to_string()))
    }
}

/// A converter that shells out to a user-configured program.
///
/// The program is run directly (argv, no shell) with `{in}`, `{out}` and
/// `{format}` placeholders substituted in its arguments, which avoids shell
/// injection from paths. It must write the converted file to `{out}`.
pub struct CommandConverter {
    program: String,
    args: Vec<String>,
    timeout: Duration,
}

impl CommandConverter {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Override the process timeout (default [`DEFAULT_TIMEOUT`]).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn substitute(&self, template: &str, input: &Path, output: &Path, target: &str) -> String {
        template
            .replace("{in}", &input.to_string_lossy())
            .replace("{out}", &output.to_string_lossy())
            .replace("{format}", target)
    }
}

impl Converter for CommandConverter {
    fn id(&self) -> String {
        format!("command:{}", self.program)
    }

    fn available(&self) -> bool {
        true
    }

    fn convert(&self, target: &str, input_name: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        let dir = tempfile::tempdir()?;
        let ext = Path::new(input_name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");
        let input = dir.path().join(format!("input.{ext}"));
        let output = dir.path().join(format!("output.{target}"));
        std::fs::write(&input, bytes)?;

        // Redirect stderr to a scratch file rather than a pipe: a chatty
        // converter must not deadlock us while we poll for completion.
        let stderr_path = dir.path().join("stderr.log");
        let stderr = std::fs::File::create(&stderr_path)?;

        let args: Vec<String> = self
            .args
            .iter()
            .map(|arg| self.substitute(arg, &input, &output, target))
            .collect();

        let mut command = Command::new(&self.program);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr));

        let mut child = command.spawn().map_err(|e| {
            Error::Conversion(format!("failed to run converter '{}': {e}", self.program))
        })?;

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait()? {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::Conversion(format!(
                        "converter '{}' timed out after {:?}",
                        self.program, self.timeout
                    )));
                }
                None => std::thread::sleep(Duration::from_millis(25)),
            }
        };

        if !status.success() {
            let mut log = String::new();
            if let Ok(mut file) = std::fs::File::open(&stderr_path) {
                let _ = file.read_to_string(&mut log);
            }
            let log = log.trim();
            return Err(Error::Conversion(if log.is_empty() {
                format!("converter '{}' exited with {status}", self.program)
            } else {
                format!("converter '{}' failed: {log}", self.program)
            }));
        }

        match std::fs::read(&output) {
            Ok(out) if !out.is_empty() => Ok(out),
            Ok(_) => Err(Error::Conversion(format!(
                "converter '{}' produced an empty '{target}' file",
                self.program
            ))),
            Err(e) => Err(Error::Conversion(format!(
                "converter '{}' did not produce an output file: {e}",
                self.program
            ))),
        }
    }
}

/// Content-addressed cache in front of another [`Converter`].
///
/// The key is the git blob oid of the input plus the inner converter's
/// [`Converter::id`] and the target format, so identical parts convert once and
/// switching converters naturally misses the old entries.
pub struct CachedConverter {
    inner: Arc<dyn Converter>,
    root: PathBuf,
}

impl CachedConverter {
    /// `root` is the cache directory (e.g. `<project>/.openpdm/cache`).
    pub fn new(inner: Arc<dyn Converter>, root: impl Into<PathBuf>) -> Self {
        Self {
            inner,
            root: root.into(),
        }
    }

    fn slug(id: &str) -> String {
        let mut out: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        if out.is_empty() {
            out.push_str("converter");
        }
        out
    }
}

impl Converter for CachedConverter {
    fn id(&self) -> String {
        self.inner.id()
    }

    fn available(&self) -> bool {
        self.inner.available()
    }

    fn convert(&self, target: &str, input_name: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        let key = Oid::hash_object(ObjectType::Blob, bytes)
            .map_err(|e| Error::Conversion(format!("hashing input: {e}")))?
            .to_string();
        let dir = self.root.join(target).join(Self::slug(&self.inner.id()));
        let file = dir.join(format!("{key}.{target}"));

        if let Ok(cached) = std::fs::read(&file) {
            if !cached.is_empty() {
                return Ok(cached);
            }
        }

        let produced = self.inner.convert(target, input_name, bytes)?;
        std::fs::create_dir_all(&dir)?;
        // Write via a temp file + rename so a concurrent reader never sees a
        // half-written artifact.
        let tmp = dir.join(format!("{key}.tmp"));
        std::fs::write(&tmp, &produced)?;
        std::fs::rename(&tmp, &file)?;
        Ok(produced)
    }
}

/// `converters.json` as read from `<root>/.openpdm/converters.json`.
///
/// The file maps a file extension (case-insensitive, no dot) to the program
/// that converts it:
///
/// ```json
/// { "converters": { "sldprt": { "program": "to-stl", "args": ["{in}", "{out}"] } } }
/// ```
#[derive(Deserialize)]
struct ConvertersConfig {
    #[serde(default)]
    converters: BTreeMap<String, CommandSpec>,
}

#[derive(Deserialize)]
struct CommandSpec {
    program: String,
    #[serde(default)]
    args: Vec<String>,
}

/// Per-format external converters loaded from a project's `converters.json`.
///
/// Backends are keyed by lower-case file extension, so a single
/// [`ConvertibleHandler`](crate::handlers::converted::ConvertibleHandler) can
/// route any allowlisted proprietary format to whichever engine the user has
/// configured (SolidWorks, Inventor, Creo/NX, CATIA, kernel formats, …).
pub struct ConverterRegistry {
    backends: BTreeMap<String, Arc<dyn Converter>>,
}

impl ConverterRegistry {
    /// A registry with no configured backends; every format degrades to
    /// metadata only.
    pub fn empty() -> Self {
        Self {
            backends: BTreeMap::new(),
        }
    }

    /// Build a registry from explicit `(extension, converter)` pairs.
    pub fn from_backends<I>(backends: I) -> Self
    where
        I: IntoIterator<Item = (String, Arc<dyn Converter>)>,
    {
        Self {
            backends: backends
                .into_iter()
                .map(|(format, converter)| (format.to_ascii_lowercase(), converter))
                .collect(),
        }
    }

    /// Load a project's converters from `<root>/.openpdm/converters.json`.
    ///
    /// The config lives beside the (gitignored) SQLite index and is machine
    /// local. A missing or malformed file yields an empty registry, so
    /// proprietary files degrade to metadata only.
    pub fn load(root: &Path) -> Self {
        let config_path = root.join(".openpdm").join("converters.json");
        let Ok(text) = std::fs::read_to_string(&config_path) else {
            return Self::empty();
        };
        let Ok(config) = serde_json::from_str::<ConvertersConfig>(&text) else {
            return Self::empty();
        };
        let cache_root = root.join(".openpdm").join("cache");
        let backends = config
            .converters
            .into_iter()
            .filter(|(_, spec)| !spec.program.is_empty())
            .map(|(format, spec)| {
                let backend: Arc<dyn Converter> = Arc::new(CachedConverter::new(
                    Arc::new(CommandConverter::new(spec.program, spec.args)),
                    cache_root.clone(),
                ));
                (format.to_ascii_lowercase(), backend)
            })
            .collect();
        Self { backends }
    }

    /// The converter configured for `format` (case-insensitive), if any.
    pub fn converter_for(&self, format: &str) -> Option<Arc<dyn Converter>> {
        self.backends.get(&format.to_ascii_lowercase()).cloned()
    }

    /// Whether any backend is configured.
    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    /// Configured formats (lower-case extensions), for introspection.
    pub fn formats(&self) -> impl Iterator<Item = &str> {
        self.backends.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Converter that records how often it runs and echoes fixed bytes.
    struct CountingConverter {
        calls: Arc<AtomicUsize>,
        output: Vec<u8>,
    }

    impl Converter for CountingConverter {
        fn id(&self) -> String {
            "counting".to_string()
        }
        fn available(&self) -> bool {
            true
        }
        fn convert(&self, _target: &str, _input_name: &str, _bytes: &[u8]) -> Result<Vec<u8>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.output.clone())
        }
    }

    #[test]
    fn null_converter_is_unavailable() {
        let converter = NullConverter;
        assert!(!converter.available());
        assert!(converter.convert(TARGET_STL, "part.sldprt", b"x").is_err());
    }

    #[test]
    fn cache_hits_on_identical_input() {
        let calls = Arc::new(AtomicUsize::new(0));
        let dir = tempfile::tempdir().unwrap();
        let converter = CachedConverter::new(
            Arc::new(CountingConverter {
                calls: calls.clone(),
                output: b"mesh-bytes".to_vec(),
            }),
            dir.path(),
        );

        let first = converter.convert(TARGET_STL, "a.sldprt", b"geometry").unwrap();
        let second = converter.convert(TARGET_STL, "b.sldprt", b"geometry").unwrap();

        assert_eq!(first, b"mesh-bytes");
        assert_eq!(second, b"mesh-bytes");
        // Same content, different name: converted once.
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cache_misses_on_different_input() {
        let calls = Arc::new(AtomicUsize::new(0));
        let dir = tempfile::tempdir().unwrap();
        let converter = CachedConverter::new(
            Arc::new(CountingConverter {
                calls: calls.clone(),
                output: b"mesh".to_vec(),
            }),
            dir.path(),
        );

        converter.convert(TARGET_STL, "a.sldprt", b"one").unwrap();
        converter.convert(TARGET_STL, "a.sldprt", b"two").unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[cfg(unix)]
    #[test]
    fn command_converter_writes_output() {
        // A shell one-liner stands in for a real CAD converter: it ignores the
        // input and writes a minimal ASCII STL to `{out}`.
        let script = "printf 'solid s\\nfacet normal 0 0 1\\nouter loop\\nvertex 0 0 0\\n\
                      vertex 1 0 0\\nvertex 0 1 0\\nendloop\\nendfacet\\nendsolid s\\n' > \"$1\"";
        let converter = CommandConverter::new("sh", vec!["-c".into(), script.into(), "sh".into(), "{out}".into()]);

        let out = converter.convert(TARGET_STL, "part.sldprt", b"ignored").unwrap();
        assert!(out.starts_with(b"solid"));
    }

    #[cfg(unix)]
    #[test]
    fn command_converter_reports_failure() {
        let converter = CommandConverter::new("sh", vec!["-c".into(), "echo boom >&2; exit 3".into()]);
        let err = converter
            .convert(TARGET_STL, "part.sldprt", b"x")
            .unwrap_err()
            .to_string();
        assert!(err.contains("boom"), "unexpected error: {err}");
    }

    #[test]
    fn registry_loads_per_format_backends() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".openpdm")).unwrap();
        std::fs::write(
            dir.path().join(".openpdm").join("converters.json"),
            r#"{ "converters": {
                "sldprt": { "program": "sw", "args": ["{in}", "{out}"] },
                "PRT": { "program": "creo" }
            } }"#,
        )
        .unwrap();

        let registry = ConverterRegistry::load(dir.path());
        assert!(!registry.is_empty());
        assert_eq!(registry.converter_for("sldprt").unwrap().id(), "command:sw");
        // Lookup is case-insensitive.
        assert!(registry.converter_for("SLDPRT").is_some());
        assert_eq!(registry.converter_for("prt").unwrap().id(), "command:creo");
        assert!(registry.converter_for("catpart").is_none());
    }

    #[test]
    fn registry_without_config_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(ConverterRegistry::load(dir.path()).is_empty());
    }
}
