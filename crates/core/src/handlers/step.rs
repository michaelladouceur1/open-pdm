//! STEP (ISO 10303-21) handler.
//!
//! STEP is an exact B-rep exchange format, not a triangle mesh, and the core
//! deliberately has no geometry kernel. So, like FreeCAD `.FCStd`, this handler
//! reports metadata (parsed from the exchange header) and advertises
//! `tessellation = step`; the desktop viewer then feeds the raw bytes to
//! `occt-import-js` (OpenCASCADE in WebAssembly), which tessellates them in the
//! webview. Unlike `.FCStd` there is nothing to unpack — the `.step`/`.stp`
//! file *is* the payload.

use std::collections::BTreeMap;

use crate::Result;

use super::{basic_attributes, CadHandler, FileMetadata, ShapePayload};

/// STEP handler: `.step` / `.stp`.
pub struct StepHandler;

impl CadHandler for StepHandler {
    fn name(&self) -> &'static str {
        "step"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["step", "stp"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let mut attributes = basic_attributes(path, bytes);
        for (key, value) in parse_header(bytes) {
            attributes.insert(key, value);
        }
        // Tell the viewer to request the payload and tessellate it as STEP.
        attributes.insert("tessellation".into(), "step".into());
        Ok(FileMetadata {
            format: "step".into(),
            attributes,
        })
    }

    fn shape_payloads(&self, path: &str, bytes: &[u8]) -> Result<Vec<ShapePayload>> {
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![ShapePayload {
            name: super::payload_name(path),
            format: "step".into(),
            data: bytes.to_vec(),
        }])
    }
}

/// Extract the interesting fields from the STEP exchange header.
///
/// The header is a handful of records — `FILE_DESCRIPTION`, `FILE_NAME`,
/// `FILE_SCHEMA` — near the top of the file; the body is not touched except for
/// an entity count.
fn parse_header(bytes: &[u8]) -> BTreeMap<String, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut attributes = BTreeMap::new();

    if let Some(args) = header_record(&text, "FILE_DESCRIPTION") {
        insert_nonempty(&mut attributes, "description", arg(&args, 0));
    }
    if let Some(args) = header_record(&text, "FILE_NAME") {
        insert_nonempty(&mut attributes, "file_name", arg(&args, 0));
        insert_nonempty(&mut attributes, "time_stamp", arg(&args, 1));
        insert_nonempty(&mut attributes, "author", arg(&args, 2));
        insert_nonempty(&mut attributes, "organization", arg(&args, 3));
        insert_nonempty(&mut attributes, "preprocessor_version", arg(&args, 4));
        insert_nonempty(&mut attributes, "originating_system", arg(&args, 5));
    }
    if let Some(args) = header_record(&text, "FILE_SCHEMA") {
        if let Some(raw) = args.first() {
            let schema = schema_name(raw);
            insert_nonempty(&mut attributes, "schema", &schema);
        }
    }

    let entities = count_entities(&text);
    if entities > 0 {
        attributes.insert("entities".into(), entities.to_string());
    }

    attributes
}

/// The `index`-th argument of a header record, or `""`.
fn arg<'a>(args: &'a [String], index: usize) -> &'a str {
    args.get(index).map(String::as_str).unwrap_or("")
}

fn insert_nonempty(attributes: &mut BTreeMap<String, String>, key: &str, value: &str) {
    if !value.is_empty() {
        attributes.insert(key.to_string(), value.to_string());
    }
}

/// Find `NAME(...)` in the text and return its top-level arguments.
///
/// Nested lists are flattened and their quoted strings concatenated, which is
/// exactly what the header fields are: `FILE_NAME('a',('x','y'),…)` yields
/// `["a", "x, y", …]`.
fn header_record(text: &str, name: &str) -> Option<Vec<String>> {
    let start = text.find(name)?;
    let after = &text[start + name.len()..];
    let open = after.find('(')?;
    let body = &after[open + 1..];
    let end = matching_paren(body)?;
    Some(split_arguments(&body[..end]))
}

/// Index of the `)` matching the already-consumed `(`, honouring quotes.
fn matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1usize;
    let mut in_quote = false;
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        if in_quote {
            if c == '\'' {
                // A doubled quote is an escaped quote, not a terminator.
                if s[i + 1..].starts_with('\'') {
                    chars.next();
                } else {
                    in_quote = false;
                }
            }
            continue;
        }
        match c {
            '\'' => in_quote = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a STEP argument list on top-level commas, stripping quotes and
/// collapsing whitespace. Nested list commas become `", "` so multi-valued
/// fields read naturally.
fn split_arguments(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quote {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    current.push('\'');
                } else {
                    in_quote = false;
                }
            } else {
                current.push(c);
            }
            continue;
        }
        match c {
            '\'' => in_quote = true,
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => args.push(std::mem::take(&mut current)),
            ',' => current.push_str(", "),
            _ => current.push(c),
        }
    }
    args.push(current);

    args.into_iter()
        .map(|arg| arg.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

/// Reduce a `FILE_SCHEMA` value to its bare name, dropping the `{ … }` version
/// block (e.g. `AUTOMOTIVE_DESIGN { 1 0 10303 214 … }` -> `AUTOMOTIVE_DESIGN`).
fn schema_name(raw: &str) -> String {
    raw.split('{').next().unwrap_or(raw).trim().to_string()
}

/// Count `#123 = …` entity instances in the data section.
fn count_entities(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let line = line.trim_start();
            let Some(rest) = line.strip_prefix('#') else {
                return false;
            };
            let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            digits > 0 && rest[digits..].trim_start().starts_with('=')
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP_FIXTURE: &str = "ISO-10303-21;\n\
HEADER;\n\
FILE_DESCRIPTION(('Open CASCADE Shape Model'),'2;1');\n\
FILE_NAME('cube.step','2026-01-02T03:04:05',('Ada Lovelace'),('ACME'),'Open CASCADE','OCCT','');\n\
FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 3 1 1 }'));\n\
ENDSEC;\n\
DATA;\n\
#1 = CARTESIAN_POINT('',(0.,0.,0.));\n\
#2 = CARTESIAN_POINT('',(1.,0.,0.));\n\
#3 = CARTESIAN_POINT('',(0.,1.,0.));\n\
ENDSEC;\n\
END-ISO-10303-21;\n";

    #[test]
    fn parses_header_metadata() {
        let meta = StepHandler.metadata("cube.step", STEP_FIXTURE.as_bytes()).unwrap();
        assert_eq!(meta.format, "step");
        assert_eq!(
            meta.attributes.get("description").map(String::as_str),
            Some("Open CASCADE Shape Model")
        );
        assert_eq!(
            meta.attributes.get("file_name").map(String::as_str),
            Some("cube.step")
        );
        assert_eq!(
            meta.attributes.get("time_stamp").map(String::as_str),
            Some("2026-01-02T03:04:05")
        );
        assert_eq!(
            meta.attributes.get("author").map(String::as_str),
            Some("Ada Lovelace")
        );
        assert_eq!(
            meta.attributes.get("organization").map(String::as_str),
            Some("ACME")
        );
        assert_eq!(
            meta.attributes.get("originating_system").map(String::as_str),
            Some("OCCT")
        );
        assert_eq!(
            meta.attributes.get("schema").map(String::as_str),
            Some("AUTOMOTIVE_DESIGN")
        );
        assert_eq!(meta.attributes.get("entities").map(String::as_str), Some("3"));
        assert_eq!(
            meta.attributes.get("tessellation").map(String::as_str),
            Some("step")
        );
    }

    #[test]
    fn exposes_the_file_as_a_step_payload() {
        let payloads = StepHandler
            .shape_payloads("parts/cube.stp", STEP_FIXTURE.as_bytes())
            .unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].name, "cube.stp");
        assert_eq!(payloads[0].format, "step");
        assert_eq!(payloads[0].data, STEP_FIXTURE.as_bytes());
    }

    #[test]
    fn empty_file_has_no_payload() {
        assert!(StepHandler.shape_payloads("empty.step", b"").unwrap().is_empty());
    }

    #[test]
    fn tolerates_a_file_without_a_header() {
        // A malformed/minimal file still resolves and advertises the codec; the
        // viewer's tessellator decides whether it is renderable.
        let meta = StepHandler.metadata("broken.stp", b"not really step").unwrap();
        assert_eq!(meta.format, "step");
        assert_eq!(
            meta.attributes.get("tessellation").map(String::as_str),
            Some("step")
        );
    }
}
