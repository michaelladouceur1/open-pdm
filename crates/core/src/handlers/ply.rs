//! Stanford PLY (Polygon File Format) handler.
//!
//! PLY is a self-describing mesh format: an ASCII header declares the elements
//! (typically `vertex` and `face`) and their typed properties, followed by the
//! element data in one of three encodings — `ascii`, `binary_little_endian` or
//! `binary_big_endian`. Polygon faces are fan-triangulated, so unlike STEP or
//! FreeCAD B-rep the geometry maps straight onto a [`Mesh`] with no external
//! converter and no WebAssembly kernel.
//!
//! The parser is deliberately tolerant: unknown header directives are ignored,
//! extra vertex properties (normals, colours, …) are consumed so binary element
//! alignment is preserved, and non-`vertex`/`face` elements are skipped but
//! still walked. Faces whose indices fall outside the vertex array are dropped
//! rather than trusted.

use crate::{Error, Mesh, Result};

use super::{basic_attributes, fmt_vec3, CadHandler, FileMetadata};

/// PLY scalar property types (aliases collapsed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlyType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl PlyType {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "char" | "int8" => Self::I8,
            "uchar" | "uint8" => Self::U8,
            "short" | "int16" => Self::I16,
            "ushort" | "uint16" => Self::U16,
            "int" | "int32" => Self::I32,
            "uint" | "uint32" => Self::U32,
            "float" | "float32" => Self::F32,
            "double" | "float64" => Self::F64,
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::F64 => 8,
        }
    }

    fn decode(self, bytes: &[u8], little: bool) -> f64 {
        macro_rules! read {
            ($width:literal, $int:ty) => {{
                let arr: [u8; $width] = bytes.try_into().unwrap();
                (if little {
                    <$int>::from_le_bytes(arr)
                } else {
                    <$int>::from_be_bytes(arr)
                }) as f64
            }};
        }
        match self {
            Self::I8 => bytes[0] as i8 as f64,
            Self::U8 => bytes[0] as f64,
            Self::I16 => read!(2, i16),
            Self::U16 => read!(2, u16),
            Self::I32 => read!(4, i32),
            Self::U32 => read!(4, u32),
            Self::F32 => read!(4, f32),
            Self::F64 => read!(8, f64),
        }
    }
}

/// One declared element property.
enum Property {
    Scalar { name: String, ty: PlyType },
    List { count_ty: PlyType, item_ty: PlyType },
}

impl Property {
    fn name(&self) -> &str {
        match self {
            Self::Scalar { name, .. } => name,
            // The list's own name is unused: a `face` element has a single list.
            Self::List { .. } => "vertex_indices",
        }
    }
}

/// One declared element (`vertex`, `face`, …) with its ordered properties.
struct Element {
    name: String,
    count: usize,
    properties: Vec<Property>,
}

/// The three PLY encodings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Ascii,
    BinaryLittle,
    BinaryBig,
}

impl Encoding {
    fn name(self) -> &'static str {
        match self {
            Self::Ascii => "ascii",
            Self::BinaryLittle => "binary_little_endian",
            Self::BinaryBig => "binary_big_endian",
        }
    }
}

/// Everything a handler needs, produced by a single pass over the file.
struct PlyDocument {
    mesh: Mesh,
    encoding: &'static str,
    elements: Vec<(String, usize)>,
    comments: Vec<String>,
    vertex_properties: Vec<String>,
    /// Number of polygon faces before triangulation.
    polygon_count: usize,
    has_normals: bool,
    has_colors: bool,
}

/// Stanford PLY handler: `.ply`.
pub struct PlyHandler;

impl CadHandler for PlyHandler {
    fn name(&self) -> &'static str {
        "ply"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ply"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata> {
        let doc = parse(bytes)?;
        let mut attributes = basic_attributes(path, bytes);
        attributes.insert("encoding".into(), doc.encoding.into());
        attributes.insert("triangles".into(), doc.mesh.triangle_count().to_string());
        attributes.insert("vertices".into(), doc.mesh.vertex_count().to_string());
        attributes.insert("polygons".into(), doc.polygon_count.to_string());
        if let Some((min, max)) = doc.mesh.bounds() {
            attributes.insert("bounds_min".into(), fmt_vec3(min));
            attributes.insert("bounds_max".into(), fmt_vec3(max));
        }
        attributes.insert("volume".into(), format!("{:.4}", doc.mesh.volume()));

        if !doc.elements.is_empty() {
            attributes.insert(
                "elements".into(),
                doc.elements
                    .iter()
                    .map(|(name, count)| format!("{name} ({count})"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if !doc.vertex_properties.is_empty() {
            attributes.insert("vertex_properties".into(), doc.vertex_properties.join(", "));
        }
        if doc.has_normals {
            attributes.insert("normals".into(), "yes".into());
        }
        if doc.has_colors {
            attributes.insert("vertex_colors".into(), "yes".into());
        }
        if !doc.comments.is_empty() {
            attributes.insert(
                "comments".into(),
                doc.comments
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
        }

        Ok(FileMetadata {
            format: "ply".into(),
            attributes,
        })
    }

    fn mesh(&self, _path: &str, bytes: &[u8]) -> Result<Option<Mesh>> {
        Ok(Some(parse(bytes)?.mesh))
    }
}

/// Parse a whole PLY file into a document summary plus a triangle mesh.
fn parse(bytes: &[u8]) -> Result<PlyDocument> {
    let (header, body) = split_header(bytes)?;
    let (encoding, elements, comments) = parse_header(header)?;
    let mut reader = Body::new(encoding, body);

    let mut positions: Vec<f32> = Vec::new();
    let mut faces: Vec<Vec<u32>> = Vec::new();
    let mut vertex_properties = Vec::new();
    let mut has_normals = false;
    let mut has_colors = false;

    for element in &elements {
        let is_vertex = matches!(element.name.as_str(), "vertex" | "vertices");
        let is_face = matches!(element.name.as_str(), "face" | "faces");

        if is_vertex {
            let names: Vec<&str> = element.properties.iter().map(Property::name).collect();
            vertex_properties = names.iter().map(|n| n.to_string()).collect();
            has_normals = names.contains(&"nx") && names.contains(&"ny") && names.contains(&"nz");
            has_colors =
                names.contains(&"red") && names.contains(&"green") && names.contains(&"blue");
        }

        for _ in 0..element.count {
            let mut xyz = [0f64; 3];
            let mut face_indices: Option<Vec<u32>> = None;

            for property in &element.properties {
                match property {
                    Property::Scalar { name, ty } => {
                        let value = reader.read_scalar(*ty)?;
                        if is_vertex {
                            match name.as_str() {
                                "x" => xyz[0] = value,
                                "y" => xyz[1] = value,
                                "z" => xyz[2] = value,
                                _ => {}
                            }
                        }
                    }
                    Property::List {
                        count_ty, item_ty, ..
                    } => {
                        // Only a face's first list property carries the indices;
                        // anything else is consumed to keep binary alignment.
                        let keep = is_face && face_indices.is_none();
                        let items = reader.read_list(*count_ty, *item_ty, keep)?;
                        if keep {
                            face_indices = Some(items);
                        }
                    }
                }
            }

            if is_vertex {
                positions.extend_from_slice(&[xyz[0] as f32, xyz[1] as f32, xyz[2] as f32]);
            }
            if is_face {
                if let Some(indices) = face_indices {
                    faces.push(indices);
                }
            }
        }
    }

    let vertex_count = positions.len() / 3;
    let mut indices: Vec<u32> = Vec::new();
    let mut polygon_count = 0usize;
    for face in &faces {
        if face.len() < 3 || face.iter().any(|&i| i as usize >= vertex_count) {
            continue;
        }
        polygon_count += 1;
        // Fan-triangulate n-gons.
        for k in 1..face.len() - 1 {
            indices.push(face[0]);
            indices.push(face[k]);
            indices.push(face[k + 1]);
        }
    }

    Ok(PlyDocument {
        mesh: Mesh::new(positions, indices),
        encoding: encoding.name(),
        elements: elements
            .iter()
            .map(|e| (e.name.clone(), e.count))
            .collect(),
        comments,
        vertex_properties,
        polygon_count,
        has_normals,
        has_colors,
    })
}

/// Split the ASCII header from the (possibly binary) body.
///
/// The header is a sequence of `\n`-terminated ASCII lines ending in
/// `end_header`; scanning byte-wise keeps the body opaque.
fn split_header(bytes: &[u8]) -> Result<(&str, &[u8])> {
    let mut start = 0usize;
    loop {
        let Some(rel) = bytes[start..].iter().position(|&b| b == b'\n') else {
            return Err(Error::parse(
                "ply",
                "header is not terminated by `end_header`",
            ));
        };
        let line_end = start + rel;
        if trim(&bytes[start..line_end]) == b"end_header" {
            let text = std::str::from_utf8(&bytes[..start])
                .map_err(|_| Error::parse("ply", "header is not valid ASCII"))?;
            return Ok((text, &bytes[(line_end + 1).min(bytes.len())..]));
        }
        start = line_end + 1;
        if start >= bytes.len() {
            return Err(Error::parse(
                "ply",
                "header is not terminated by `end_header`",
            ));
        }
    }
}

/// Parse the header into the encoding, element declarations and comments.
fn parse_header(text: &str) -> Result<(Encoding, Vec<Element>, Vec<String>)> {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    if lines.next() != Some("ply") {
        return Err(Error::parse("ply", "missing `ply` magic"));
    }

    let mut encoding = None;
    let mut elements: Vec<Element> = Vec::new();
    let mut comments = Vec::new();

    for line in lines {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("format") => {
                encoding = Some(match parts.next() {
                    Some("ascii") => Encoding::Ascii,
                    Some("binary_little_endian") => Encoding::BinaryLittle,
                    Some("binary_big_endian") => Encoding::BinaryBig,
                    other => {
                        return Err(Error::parse(
                            "ply",
                            format!("unknown format `{}`", other.unwrap_or("")),
                        ))
                    }
                });
            }
            Some("comment") => comments.push(line["comment".len()..].trim().to_string()),
            Some("element") => {
                let name = parts
                    .next()
                    .ok_or_else(|| Error::parse("ply", "`element` without a name"))?;
                let count = parts
                    .next()
                    .ok_or_else(|| Error::parse("ply", "`element` without a count"))?
                    .parse::<usize>()
                    .map_err(|_| {
                        Error::parse("ply", format!("invalid count for element `{name}`"))
                    })?;
                elements.push(Element {
                    name: name.to_string(),
                    count,
                    properties: Vec::new(),
                });
            }
            Some("property") => {
                let element = elements
                    .last_mut()
                    .ok_or_else(|| Error::parse("ply", "`property` before any `element`"))?;
                let ty = parts
                    .next()
                    .ok_or_else(|| Error::parse("ply", "`property` without a type"))?;
                if ty == "list" {
                    let count_ty = parse_ply_type(parts.next())?;
                    let item_ty = parse_ply_type(parts.next())?;
                    parts
                        .next()
                        .ok_or_else(|| Error::parse("ply", "list `property` without a name"))?;
                    element.properties.push(Property::List { count_ty, item_ty });
                } else {
                    let ty = PlyType::parse(ty).ok_or_else(|| {
                        Error::parse("ply", format!("unknown property type `{ty}`"))
                    })?;
                    let name = parts
                        .next()
                        .ok_or_else(|| Error::parse("ply", "`property` without a name"))?;
                    element.properties.push(Property::Scalar {
                        name: name.to_string(),
                        ty,
                    });
                }
            }
            // `obj_info` and anything else: ignored.
            _ => {}
        }
    }

    let encoding = encoding.ok_or_else(|| Error::parse("ply", "header has no `format` line"))?;
    Ok((encoding, elements, comments))
}

fn parse_ply_type(name: Option<&str>) -> Result<PlyType> {
    let name = name.ok_or_else(|| Error::parse("ply", "`property list` is missing a type"))?;
    PlyType::parse(name)
        .ok_or_else(|| Error::parse("ply", format!("unknown property type `{name}`")))
}

/// Seekable reader over the element data, for both ASCII and binary encodings.
struct Body<'a> {
    bytes: &'a [u8],
    offset: usize,
    /// `None` for ASCII, `Some(little_endian)` for binary.
    binary: Option<bool>,
}

impl<'a> Body<'a> {
    fn new(encoding: Encoding, bytes: &'a [u8]) -> Self {
        let binary = match encoding {
            Encoding::Ascii => None,
            Encoding::BinaryLittle => Some(true),
            Encoding::BinaryBig => Some(false),
        };
        Self {
            bytes,
            offset: 0,
            binary,
        }
    }

    /// Read one scalar of `ty`, decoded to `f64`.
    fn read_scalar(&mut self, ty: PlyType) -> Result<f64> {
        match self.binary {
            None => {
                let bytes = self.bytes;
                let mut offset = self.offset;
                while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
                    offset += 1;
                }
                let start = offset;
                while offset < bytes.len() && !bytes[offset].is_ascii_whitespace() {
                    offset += 1;
                }
                self.offset = offset;
                if start == offset {
                    return Err(Error::parse("ply", "unexpected end of ASCII element data"));
                }
                let token = std::str::from_utf8(&bytes[start..offset])
                    .map_err(|_| Error::parse("ply", "invalid ASCII element data"))?;
                token
                    .parse::<f64>()
                    .map_err(|_| Error::parse("ply", format!("invalid number `{token}`")))
            }
            Some(little) => {
                let end = self.offset + ty.size();
                if end > self.bytes.len() {
                    return Err(Error::parse("ply", "unexpected end of binary element data"));
                }
                let value = ty.decode(&self.bytes[self.offset..end], little);
                self.offset = end;
                Ok(value)
            }
        }
    }

    /// Read a `property list`: a count followed by that many items. Items are
    /// collected only when `keep` is set (i.e. a face's index list).
    fn read_list(&mut self, count_ty: PlyType, item_ty: PlyType, keep: bool) -> Result<Vec<u32>> {
        let count = self.read_scalar(count_ty)?;
        if !count.is_finite() || count < 0.0 {
            return Err(Error::parse("ply", "negative list length"));
        }
        let count = count as usize;
        // Cap the pre-allocation: a malformed count must not OOM us before the
        // reads fail on exhausted data.
        let mut items = Vec::with_capacity(if keep { count.min(4096) } else { 0 });
        for _ in 0..count {
            let value = self.read_scalar(item_ty)?;
            if keep {
                items.push(value.max(0.0) as u32);
            }
        }
        Ok(items)
    }
}

/// Strip leading and trailing ASCII whitespace from a header line.
fn trim(line: &[u8]) -> &[u8] {
    let mut slice = line;
    while let Some((first, rest)) = slice.split_first() {
        if first.is_ascii_whitespace() {
            slice = rest;
        } else {
            break;
        }
    }
    while let Some((last, rest)) = slice.split_last() {
        if last.is_ascii_whitespace() {
            slice = rest;
        } else {
            break;
        }
    }
    slice
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASCII_PLY: &str = "ply\n\
format ascii 1.0\n\
comment created by test\n\
element vertex 4\n\
property float x\n\
property float y\n\
property float z\n\
element face 2\n\
property list uchar int vertex_indices\n\
end_header\n\
0 0 0\n\
1 0 0\n\
1 1 0\n\
0 1 0\n\
3 0 1 2\n\
3 0 2 3\n";

    /// Binary PLY with a vertex element that also carries nx/ny/nz, so the
    /// face data is offset by the extra properties.
    fn binary_ply(little: bool) -> Vec<u8> {
        let endian = if little {
            "binary_little_endian"
        } else {
            "binary_big_endian"
        };
        let mut out = Vec::new();
        out.extend_from_slice(
            format!(
                "ply\nformat {endian} 1.0\nelement vertex 3\n\
                 property float x\nproperty float y\nproperty float z\n\
                 property float nx\nproperty float ny\nproperty float nz\n\
                 element face 1\nproperty list uchar int vertex_indices\nend_header\n"
            )
            .as_bytes(),
        );

        let verts: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        for v in verts {
            // x, y, z then normals (0, 0, 1).
            for f in [v[0], v[1], v[2], 0.0, 0.0, 1.0] {
                out.extend_from_slice(&if little { f.to_le_bytes() } else { f.to_be_bytes() });
            }
        }
        out.push(3u8);
        for i in [0i32, 1, 2] {
            out.extend_from_slice(&if little { i.to_le_bytes() } else { i.to_be_bytes() });
        }
        out
    }

    #[test]
    fn parses_ascii_mesh() {
        let mesh = PlyHandler.mesh("q.ply", ASCII_PLY.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.vertex_count(), 4);
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.bounds(), Some(([0.0, 0.0, 0.0], [1.0, 1.0, 0.0])));
    }

    #[test]
    fn extracts_metadata() {
        let meta = PlyHandler.metadata("q.ply", ASCII_PLY.as_bytes()).unwrap();
        assert_eq!(meta.format, "ply");
        assert_eq!(meta.attributes.get("encoding").map(String::as_str), Some("ascii"));
        assert_eq!(meta.attributes.get("vertices").map(String::as_str), Some("4"));
        assert_eq!(meta.attributes.get("triangles").map(String::as_str), Some("2"));
        assert_eq!(meta.attributes.get("polygons").map(String::as_str), Some("2"));
        assert_eq!(
            meta.attributes.get("comments").map(String::as_str),
            Some("created by test")
        );
    }

    #[test]
    fn parses_binary_little_endian() {
        let bytes = binary_ply(true);
        let meta = PlyHandler.metadata("b.ply", &bytes).unwrap();
        assert_eq!(
            meta.attributes.get("encoding").map(String::as_str),
            Some("binary_little_endian")
        );
        assert_eq!(meta.attributes.get("normals").map(String::as_str), Some("yes"));

        // The extra nx/ny/nz properties must not desynchronise the face read.
        let mesh = PlyHandler.mesh("b.ply", &bytes).unwrap().unwrap();
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn parses_binary_big_endian() {
        let bytes = binary_ply(false);
        let meta = PlyHandler.metadata("b.ply", &bytes).unwrap();
        assert_eq!(
            meta.attributes.get("encoding").map(String::as_str),
            Some("binary_big_endian")
        );
        let mesh = PlyHandler.mesh("b.ply", &bytes).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.bounds(), Some(([0.0, 0.0, 0.0], [1.0, 1.0, 0.0])));
    }

    #[test]
    fn fan_triangulates_polygons() {
        let ply = "ply\nformat ascii 1.0\nelement vertex 4\n\
                   property float x\nproperty float y\nproperty float z\n\
                   element face 1\nproperty list uchar int vertex_indices\n\
                   end_header\n\
                   0 0 0\n1 0 0\n1 1 0\n0 1 0\n4 0 1 2 3\n";
        let mesh = PlyHandler.mesh("quad.ply", ply.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 2);
    }

    #[test]
    fn drops_out_of_range_face_indices() {
        let ply = "ply\nformat ascii 1.0\nelement vertex 3\n\
                   property float x\nproperty float y\nproperty float z\n\
                   element face 1\nproperty list uchar int vertex_indices\n\
                   end_header\n\
                   0 0 0\n1 0 0\n0 1 0\n3 0 1 9\n";
        let mesh = PlyHandler.mesh("bad.ply", ply.as_bytes()).unwrap().unwrap();
        assert_eq!(mesh.triangle_count(), 0);
    }

    #[test]
    fn rejects_non_ply_input() {
        let err = PlyHandler.metadata("nope.ply", b"this is not a ply").unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "unexpected: {err:?}");
    }

    #[test]
    fn rejects_unterminated_header() {
        let err = PlyHandler
            .mesh("x.ply", b"ply\nformat ascii 1.0\nelement vertex 1\n")
            .unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "unexpected: {err:?}");
    }
}
