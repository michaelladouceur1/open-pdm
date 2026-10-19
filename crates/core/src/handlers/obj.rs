use crate::{Mesh, Result};

use super::{basic_attributes, fmt_vec3, CadHandler, FileMetadata};

/// Wavefront OBJ handler (geometry only for the first pass: `v` and `f`).
pub struct ObjHandler;

impl CadHandler for ObjHandler {
    fn name(&self) -> &'static str {
        "obj"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["obj"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let mesh = parse(bytes);
        let mut attributes = basic_attributes(path, bytes);
        attributes.insert("triangles".into(), mesh.triangle_count().to_string());
        attributes.insert("vertices".into(), mesh.vertex_count().to_string());
        if let Some((min, max)) = mesh.bounds() {
            attributes.insert("bounds_min".into(), fmt_vec3(min));
            attributes.insert("bounds_max".into(), fmt_vec3(max));
        }
        Ok(FileMetadata {
            format: "obj".into(),
            attributes,
        })
    }

    fn mesh(&self, _path: &str, bytes: &[u8]) -> Result<Option<Mesh>> {
        Ok(Some(parse(bytes)))
    }
}

fn parse(bytes: &[u8]) -> Mesh {
    let text = String::from_utf8_lossy(bytes);
    let mut positions = Vec::new();
    let mut indices = Vec::new();

    for line in text.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("v ") {
            let mut parts = rest.split_whitespace();
            for _ in 0..3 {
                positions.push(parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0));
            }
        } else if let Some(rest) = line.strip_prefix("f ") {
            let vertex_count = (positions.len() / 3) as i64;
            let poly: Vec<u32> = rest
                .split_whitespace()
                .filter_map(|token| token.split('/').next())
                .filter_map(|s| s.parse::<i64>().ok())
                .map(|i| {
                    if i > 0 {
                        (i - 1) as u32
                    } else {
                        // Negative indices are relative to the end.
                        (vertex_count + i).max(0) as u32
                    }
                })
                .collect();
            // Fan-triangulate n-gons.
            for k in 1..poly.len().saturating_sub(1) {
                indices.push(poly[0]);
                indices.push(poly[k]);
                indices.push(poly[k + 1]);
            }
        }
    }

    Mesh::new(positions, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_obj_quad_as_two_triangles() {
        let text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
        let mesh = ObjHandler.mesh("q.obj", text.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.vertex_count(), 4);
        assert_eq!(mesh.triangle_count(), 2);
    }

    #[test]
    fn handles_slash_and_negative_indices() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1/1 2/2 3/3\nf -3 -2 -1\n";
        let mesh = ObjHandler.mesh("t.obj", text.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 2);
    }
}
