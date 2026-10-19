use crate::{Error, Mesh, Result};

use super::{basic_attributes, fmt_vec3, CadHandler, FileMetadata};

/// STL handler: supports both binary and ASCII STL.
pub struct StlHandler;

impl CadHandler for StlHandler {
    fn name(&self) -> &'static str {
        "stl"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["stl"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let mesh = parse(bytes)?;
        let mut attributes = basic_attributes(path, bytes);
        attributes.insert("encoding".into(), if is_ascii(bytes) { "ascii" } else { "binary" }.into());
        attributes.insert("triangles".into(), mesh.triangle_count().to_string());
        attributes.insert("vertices".into(), mesh.vertex_count().to_string());
        if let Some((min, max)) = mesh.bounds() {
            attributes.insert("bounds_min".into(), fmt_vec3(min));
            attributes.insert("bounds_max".into(), fmt_vec3(max));
        }
        attributes.insert("volume".into(), format!("{:.4}", mesh.volume()));
        Ok(FileMetadata {
            format: "stl".into(),
            attributes,
        })
    }

    fn mesh(&self, _path: &str, bytes: &[u8]) -> Result<Option<Mesh>> {
        Ok(Some(parse(bytes)?))
    }
}

fn parse(bytes: &[u8]) -> Result<Mesh> {
    if is_ascii(bytes) {
        parse_ascii(bytes)
    } else {
        parse_binary(bytes)
    }
}

/// Binary STL = 80-byte header + u32 triangle count + 50 bytes/triangle. If the
/// declared count matches the file length exactly, trust it. Otherwise fall
/// back to a "solid ... facet" sniff (ASCII files also start with `solid`).
fn is_ascii(bytes: &[u8]) -> bool {
    if bytes.len() >= 84 {
        let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        if 84 + count * 50 == bytes.len() {
            return false;
        }
    }
    let head = &bytes[..bytes.len().min(1024)];
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    text.trim_start().starts_with("solid") && text.contains("facet")
}

fn parse_binary(bytes: &[u8]) -> Result<Mesh> {
    if bytes.len() < 84 {
        return Err(Error::parse("stl", "file shorter than 84-byte header"));
    }
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    let mut positions = Vec::with_capacity(count * 9);
    let mut indices = Vec::with_capacity(count * 3);
    let mut offset = 84usize;

    for tri in 0..count {
        if offset + 50 > bytes.len() {
            return Err(Error::parse("stl", format!("truncated at triangle {tri}")));
        }
        // Bytes 0..12 are the face normal; geometry follows.
        for v in 0..3 {
            let base = offset + 12 + v * 12;
            for axis in 0..3 {
                let s = base + axis * 4;
                positions.push(f32::from_le_bytes(bytes[s..s + 4].try_into().unwrap()));
            }
            indices.push((tri * 3 + v) as u32);
        }
        offset += 50;
    }
    Ok(Mesh::new(positions, indices))
}

fn parse_ascii(bytes: &[u8]) -> Result<Mesh> {
    let text = String::from_utf8_lossy(bytes);
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    let mut next: u32 = 0;

    for line in text.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("vertex") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let mut coord = [0f32; 3];
        for c in &mut coord {
            *c = parts
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| Error::parse("stl", "malformed `vertex` line"))?;
        }
        positions.extend_from_slice(&coord);
        indices.push(next);
        next += 1;
    }
    Ok(Mesh::new(positions, indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binary_stl(tris: &[[f32; 9]]) -> Vec<u8> {
        let mut out = vec![0u8; 80];
        out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
        for t in tris {
            out.extend_from_slice(&[0f32; 3].map(|f| f.to_le_bytes()).concat());
            for v in 0..3 {
                for axis in 0..3 {
                    out.extend_from_slice(&t[v * 3 + axis].to_le_bytes());
                }
            }
            out.extend_from_slice(&0u16.to_le_bytes());
        }
        out
    }

    #[test]
    fn parses_binary_stl() {
        let bytes = binary_stl(&[[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]]);
        let handler = StlHandler;
        assert_eq!(handler.name(), "stl");
        let mesh = handler.mesh("cube.stl", &bytes).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.vertex_count(), 3);
        let meta = handler.metadata("cube.stl", &bytes).unwrap();
        assert_eq!(meta.attributes.get("triangles").map(String::as_str), Some("1"));
        assert_eq!(meta.attributes.get("encoding").map(String::as_str), Some("binary"));
    }

    #[test]
    fn parses_ascii_stl() {
        let text = "solid s\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid s\n";
        let mesh = StlHandler.mesh("s.stl", text.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 1);
    }
}
