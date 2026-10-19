use serde::{Deserialize, Serialize};

/// A simple triangle mesh, in a layout that maps directly onto a
/// `THREE.BufferGeometry` in the frontend viewer.
///
/// * `positions` is a flat `[x, y, z, x, y, z, ...]` array (3 floats/vertex).
/// * `indices` is a flat `[i, j, k, ...]` array (3 indices/triangle).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Mesh {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn new(positions: Vec<f32>, indices: Vec<u32>) -> Self {
        Self { positions, indices }
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Axis-aligned bounding box as `(min, max)`.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        if self.positions.len() < 3 {
            return None;
        }
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in self.positions.chunks_exact(3) {
            for axis in 0..3 {
                min[axis] = min[axis].min(v[axis]);
                max[axis] = max[axis].max(v[axis]);
            }
        }
        Some((min, max))
    }

    /// Signed volume via the tetrahedron decomposition around the origin
    /// (absolute value returned). Only meaningful for closed, consistently
    /// wound meshes; used as a rough sanity metric for a first pass.
    pub fn volume(&self) -> f64 {
        let mut total = 0.0f64;
        for tri in self.indices.chunks_exact(3) {
            let a = self.vertex(tri[0] as usize);
            let b = self.vertex(tri[1] as usize);
            let c = self.vertex(tri[2] as usize);
            total += dot(a, cross(b, c)) / 6.0;
        }
        total.abs()
    }

    fn vertex(&self, i: usize) -> [f64; 3] {
        let base = i * 3;
        [
            self.positions[base] as f64,
            self.positions[base + 1] as f64,
            self.positions[base + 2] as f64,
        ]
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_triangle_has_expected_shape() {
        let m = Mesh::new(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], vec![0, 1, 2]);
        assert_eq!(m.vertex_count(), 3);
        assert_eq!(m.triangle_count(), 1);
        assert_eq!(m.bounds(), Some(([0.0, 0.0, 0.0], [1.0, 1.0, 0.0])));
    }
}
