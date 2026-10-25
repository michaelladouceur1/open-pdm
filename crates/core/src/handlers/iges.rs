//! IGES (`.iges` / `.igs`) handler.
//!
//! IGES (Initial Graphics Exchange Specification) is an exact geometry exchange
//! format, not a triangle mesh, and the core deliberately has no geometry
//! kernel. Like STEP, this handler parses the fixed-column record sections for
//! metadata and advertises `tessellation = iges`; the desktop viewer then hands
//! the raw bytes to `occt-import-js`, which reads it with `ReadIgesFile` and
//! tessellates it in the webview. The file _is_ the payload, so no external
//! converter is needed.
//!
//! An IGES file is a sequence of 80-column records grouped into sections by the
//! letter in column 73: Start (`S`, free text), Global (`G`, the parameter list),
//! Directory Entry (`D`), Parameter Data (`P`) and Terminate (`T`). Only `S`,
//! `G` and `D` are read here.

use std::collections::BTreeMap;

use crate::Result;

use super::{basic_attributes, CadHandler, FileMetadata, ShapePayload};

/// IGES handler: `.iges` / `.igs`.
pub struct IgesHandler;

impl CadHandler for IgesHandler {
    fn name(&self) -> &'static str {
        "iges"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["iges", "igs"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let summary = parse(bytes);
        let params = &summary.params;
        let get = |index: usize| params.get(index).map(String::as_str).unwrap_or("");

        let mut attributes = basic_attributes(path, bytes);
        insert(&mut attributes, "product_id", get(1));
        insert(&mut attributes, "file_name", get(2));
        insert(&mut attributes, "native_system", get(3));
        insert(&mut attributes, "preprocessor_version", get(4));
        insert(&mut attributes, "receiver", get(10));
        insert(&mut attributes, "model_space_scale", get(11));
        insert(&mut attributes, "unit_flag", get(12));
        let unit = unit_name(get(12), get(13));
        insert(&mut attributes, "unit", &unit);
        insert(&mut attributes, "created", get(16));
        insert(&mut attributes, "min_resolution", get(17));
        insert(&mut attributes, "max_coordinate", get(18));
        insert(&mut attributes, "author", get(19));
        insert(&mut attributes, "organization", get(20));
        insert(&mut attributes, "drafting_standard", get(22));
        if summary.entity_count > 0 {
            attributes.insert("entities".into(), summary.entity_count.to_string());
        }
        if !summary.start.is_empty() {
            attributes.insert("start".into(), summary.start.clone());
        }
        // Tell the viewer to request the payload and tessellate it as IGES.
        attributes.insert("tessellation".into(), "iges".into());
        Ok(FileMetadata {
            format: "iges".into(),
            attributes,
        })
    }

    fn shape_payloads(&self, path: &str, bytes: &[u8]) -> Result<Vec<ShapePayload>> {
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![ShapePayload {
            name: super::payload_name(path),
            format: "iges".into(),
            data: bytes.to_vec(),
        }])
    }
}

/// What we read out of an IGES file: the decoded Global-section parameters,
/// concatenated Start-section text and the entity count.
#[derive(Default)]
struct IgesSummary {
    start: String,
    params: Vec<String>,
    entity_count: usize,
}

/// Split the file into its fixed-column sections and parse the Global list.
fn parse(bytes: &[u8]) -> IgesSummary {
    let text = String::from_utf8_lossy(bytes);
    let mut summary = IgesSummary::default();
    let mut global = String::new();
    let mut directory_records = 0usize;

    for line in text.lines() {
        let raw = line.as_bytes();
        // Section letter lives in column 73 (index 72).
        if raw.len() < 73 {
            continue;
        }
        let data = String::from_utf8_lossy(&raw[..72]);
        match raw[72] {
            b'S' | b's' => {
                let trimmed = data.trim();
                if !trimmed.is_empty() && summary.start.len() < 256 {
                    summary.start.push_str(trimmed);
                    summary.start.push(' ');
                }
            }
            b'G' | b'g' => global.push_str(&data),
            b'D' | b'd' => directory_records += 1,
            _ => {}
        }
    }

    summary.start = summary.start.trim().to_string();
    summary.params = parse_global(&global);
    // Each entity has two Directory Entry records.
    summary.entity_count = directory_records / 2;
    summary
}

/// Tokenise the Global section, honouring Hollerith (`nHtext`) strings.
///
/// The first character is the parameter delimiter; the next parameter is the
/// record delimiter, so the returned vector is indexed with `spec - 2` (e.g.
/// the sender product id is param 3 -> `params[1]`).
fn parse_global(data: &str) -> Vec<String> {
    let Some(delimiter) = data.chars().next() else {
        return Vec::new();
    };
    let bytes = data.as_bytes();
    let len = data.len();
    let mut params = Vec::new();
    let mut index = delimiter.len_utf8();

    while index < len {
        // Hollerith string: `<count>H<count characters>`.
        let mut digits = index;
        while digits < len && bytes[digits].is_ascii_digit() {
            digits += 1;
        }
        if digits > index && digits < len && matches!(bytes[digits], b'H' | b'h') {
            let count = data[index..digits].parse::<usize>().unwrap_or(0);
            let value_start = digits + 1;
            let value_end = (value_start + count).min(len);
            params.push(data[value_start..value_end].to_string());
            index = value_end;
        } else {
            let mut end = index;
            while end < len && bytes[end] as char != delimiter {
                end += 1;
            }
            params.push(data[index..end].trim().to_string());
            index = end;
        }
        if index < len && bytes[index] as char == delimiter {
            index += 1;
        }
    }

    // The record delimiter also terminates the list; drop it and any padding.
    if let Some(position) = params.iter().skip(1).position(|param| param == ";") {
        params.truncate(position + 1);
    }
    params
}

/// Resolve the unit name, falling back to the numeric unit flag.
fn unit_name(flag: &str, name: &str) -> String {
    if !name.is_empty() {
        return name.to_string();
    }
    match flag.trim() {
        "1" => "inches",
        "2" => "millimeters",
        "4" => "feet",
        "5" => "miles",
        "6" => "meters",
        "7" => "kilometers",
        "8" => "mils",
        "9" => "microns",
        "10" => "centimeters",
        "11" => "microinches",
        _ => "",
    }
    .to_string()
}

fn insert(attributes: &mut BTreeMap<String, String>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        attributes.insert(key.to_string(), value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Global-section parameter list of a small millimetre file, as it would
    /// appear (delimited by `,` and terminated by `;`).
    const GLOBAL: &str = ",;,7HOpenPDM,9Hplate.igs,6HOCCT 7,11H1.0 preproc,32,38,6,308,15,\
4HACME,1.0,2,2HMM,1,1.0,15H20260102.030405,1.0E-07,1000.0,12HAda Lovelace,4HACME,11,0,\
15H20260102.030405;";

    /// Build an ASCII IGES file: a Start line, the given Global text split across
    /// 72-column records, and a Terminate line.
    fn iges_file(global: &str) -> Vec<u8> {
        let mut out = String::new();
        out.push_str(&format!("{:<72}{}{:07}\n", "OpenPDM test file", 'S', 1));
        for (index, chunk) in global.as_bytes().chunks(72).enumerate() {
            let text = String::from_utf8_lossy(chunk);
            out.push_str(&format!("{:<72}{}{:07}\n", text, 'G', index + 1));
        }
        out.push_str(&format!(
            "{:<72}{}{:07}\n",
            "S      1G      2D      0P      0", 'T', 1
        ));
        out.into_bytes()
    }

    #[test]
    fn parses_global_metadata() {
        let bytes = iges_file(GLOBAL);
        let meta = IgesHandler.metadata("plate.igs", &bytes).unwrap();
        assert_eq!(meta.format, "iges");
        assert_eq!(meta.attributes.get("product_id").map(String::as_str), Some("OpenPDM"));
        assert_eq!(meta.attributes.get("file_name").map(String::as_str), Some("plate.igs"));
        assert_eq!(meta.attributes.get("native_system").map(String::as_str), Some("OCCT 7"));
        assert_eq!(meta.attributes.get("receiver").map(String::as_str), Some("ACME"));
        assert_eq!(meta.attributes.get("unit").map(String::as_str), Some("MM"));
        assert_eq!(meta.attributes.get("unit_flag").map(String::as_str), Some("2"));
        assert_eq!(
            meta.attributes.get("author").map(String::as_str),
            Some("Ada Lovelace")
        );
        assert_eq!(meta.attributes.get("organization").map(String::as_str), Some("ACME"));
        assert_eq!(meta.attributes.get("start").map(String::as_str), Some("OpenPDM test file"));
        assert_eq!(
            meta.attributes.get("tessellation").map(String::as_str),
            Some("iges")
        );
    }

    #[test]
    fn exposes_the_file_as_an_iges_payload() {
        let bytes = iges_file(GLOBAL);
        let payloads = IgesHandler.shape_payloads("parts/plate.igs", &bytes).unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].name, "plate.igs");
        assert_eq!(payloads[0].format, "iges");
        assert_eq!(payloads[0].data, bytes);
    }

    #[test]
    fn empty_file_has_no_payload() {
        assert!(IgesHandler.shape_payloads("empty.igs", b"").unwrap().is_empty());
    }

    #[test]
    fn tolerates_a_file_without_a_global_section() {
        // Still resolves and advertises the codec; the viewer decides.
        let meta = IgesHandler.metadata("broken.igs", b"not really iges\n").unwrap();
        assert_eq!(meta.format, "iges");
        assert_eq!(
            meta.attributes.get("tessellation").map(String::as_str),
            Some("iges")
        );
    }

    #[test]
    fn hollerith_strings_may_contain_delimiters() {
        // A comma inside a Hollerith string must not split the parameter.
        let global = ",;,5HACME,9Hpart.igs,4ABC,11H1.0 preproc,32,38,6,308,15,1.0,2,2HMM,1,1.0,\
15H20260102.030405,1.0E-07,1000.0,9HSmith, Jo,3HORG,11,0,15H20260102.030405;";
        let params = parse_global(global);
        // The comma inside the Hollerith author must not split the parameter.
        assert!(params.iter().any(|param| param == "Smith, Jo"), "got {params:?}");
    }
}
