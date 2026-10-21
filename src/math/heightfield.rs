//! Regular, immutable triangulated terrain in a caller-defined frame.
use super::{Aabb, ClosestHit, Ray, Triangle, TriangleMesh};
use glam::Vec3;

/// A finite row-major grid of height samples, interpreted as a surface.
///
/// Columns advance along `horizontal_axes().0`, rows along `.1`. The origin
/// is the zero-height position at column/row zero. `scale.x` and `scale.z`
/// are positive column/row spacings; `scale.y` is a finite signed height
/// multiplier (zero is allowed). These are local frame components, not world axes.
/// Each cell uses the diagonal from (column + 1, row) to (column, row + 1).
/// Sampling is piecewise linear on those triangles, **not** bilinear.
/// World vertices are rounded to f32; very small spacing relative to origin can
/// collapse faces, with the same numerical limits as ordinary triangles.
/// Queries are linear scans through an immutable cached [`TriangleMesh`].
///
/// ```
/// use macroquad::prelude::*;
/// let terrain = Heightfield::new(2, 2, vec![0.0, 1.0, 1.0, 2.0],
///     Vec3::ONE, Vec3::ZERO).unwrap();
/// assert_eq!(terrain.sample_height(0.25, 0.5), Some(0.75));
/// let hit = terrain.ray_intersect(&Ray::new(vec3(0.25, 3.0, 0.5), -Vec3::Y)).unwrap();
/// assert_eq!(hit.hit.point, vec3(0.25, 0.75, 0.5));
/// ```
#[derive(Clone, Debug)]
pub struct Heightfield {
    columns: usize,
    rows: usize,
    heights: Vec<f32>,
    scale: Vec3,
    origin: Vec3,
    up: Vec3,
    u: Vec3,
    v: Vec3,
    mesh: TriangleMesh,
}

impl Heightfield {
    /// Constructs a Y-up grid; its horizontal axes are +X and +Z.
    /// See [`Self::with_up`] for validation and custom frames.
    pub fn new(
        columns: usize,
        rows: usize,
        heights: Vec<f32>,
        scale: Vec3,
        origin: Vec3,
    ) -> Option<Self> {
        Self::with_up(columns, rows, heights, scale, origin, Vec3::Y)
    }

    /// Constructs a grid with normalized caller-supplied up. The column axis
    /// is world +X projected onto the horizontal plane, except when
    /// `abs(normalized_up.x) > 0.9`, when world +Z is projected instead.
    /// The row axis is `column_axis.cross(up)`; triangles wind toward up.
    ///
    /// Returns `None` for fewer than two rows/columns, mismatched/overflowing
    /// dimensions, non-finite inputs, zero up, non-positive horizontal spacing,
    /// or generated world vertices/heights outside finite f32 range. Input
    /// heights and frame are privately owned; replace the grid to edit it.
    pub fn with_up(
        columns: usize,
        rows: usize,
        heights: Vec<f32>,
        scale: Vec3,
        origin: Vec3,
        up: Vec3,
    ) -> Option<Self> {
        if columns < 2
            || rows < 2
            || columns.checked_mul(rows)? != heights.len()
            || !scale.is_finite()
            || scale.x <= 0.0
            || scale.z <= 0.0
            || !origin.is_finite()
            || !up.is_finite()
            || heights.iter().any(|h| {
                !h.is_finite()
                    || !(*h as f64 * scale.y as f64).is_finite()
                    || (*h as f64 * scale.y as f64).abs() > f32::MAX as f64
            })
        {
            return None;
        }
        if !((columns - 1) as f64 * scale.x as f64).is_finite()
            || (columns - 1) as f64 * scale.x as f64 > f32::MAX as f64
            || (rows - 1) as f64 * scale.z as f64 > f32::MAX as f64
        {
            return None;
        }
        let up64 = up.as_dvec3();
        if up64.length_squared() == 0.0 {
            return None;
        }
        let up64 = up64.normalize();
        let reference = if up64.x.abs() > 0.9 { Vec3::Z } else { Vec3::X }.as_dvec3();
        let u64 = (reference - up64 * reference.dot(up64)).normalize();
        let v64 = u64.cross(up64).normalize();
        let mut vertices = Vec::with_capacity(heights.len());
        for row in 0..rows {
            for col in 0..columns {
                let vertex = origin.as_dvec3()
                    + u64 * (col as f64 * scale.x as f64)
                    + v64 * (row as f64 * scale.z as f64)
                    + up64 * (heights[row * columns + col] as f64 * scale.y as f64);
                let vertex = vertex.as_vec3();
                if !vertex.is_finite() {
                    return None;
                }
                vertices.push(vertex);
            }
        }
        let count = (columns - 1).checked_mul(rows - 1)?.checked_mul(2)?;
        let mut triangles = Vec::with_capacity(count);
        for row in 0..rows - 1 {
            for col in 0..columns - 1 {
                let i = row * columns + col;
                let [a, b, c, d] = [
                    vertices[i],
                    vertices[i + 1],
                    vertices[i + columns],
                    vertices[i + columns + 1],
                ];
                triangles.push(Triangle::new(a, c, b));
                triangles.push(Triangle::new(b, c, d));
            }
        }
        Some(Self {
            columns,
            rows,
            heights,
            scale,
            origin,
            up: up64.as_vec3(),
            u: u64.as_vec3(),
            v: v64.as_vec3(),
            mesh: TriangleMesh::new(triangles)?,
        })
    }

    /// Number of columns and rows, respectively.
    pub fn dimensions(&self) -> (usize, usize) {
        (self.columns, self.rows)
    }
    /// Unscaled samples, indexed as `row * columns + column`.
    pub fn heights(&self) -> &[f32] {
        &self.heights
    }
    /// Local column, height and row scale.
    pub fn scale(&self) -> Vec3 {
        self.scale
    }
    /// Zero-height world position at column/row zero.
    pub fn origin(&self) -> Vec3 {
        self.origin
    }
    /// Normalized up direction.
    pub fn up(&self) -> Vec3 {
        self.up
    }
    /// Unit column and row axes in world space.
    pub fn horizontal_axes(&self) -> (Vec3, Vec3) {
        (self.u, self.v)
    }
    /// Immutable world-space surface. Triangle index is
    /// `2 * (row * (columns - 1) + column) + half`, with half 0 below
    /// the cell diagonal (`column_fraction + row_fraction <= 1`).
    pub fn mesh(&self) -> &TriangleMesh {
        &self.mesh
    }
    /// Cached tight world bounds.
    pub fn aabb(&self) -> Aabb {
        self.mesh.aabb().unwrap()
    }

    /// Scaled height above origin at horizontal distances `(u, v)` in the
    /// grid frame. Includes the final row/column edge; the outer distances are rounded to f32. Outside or non-finite
    /// coordinates return `None`. Does not clamp outside points.
    pub fn sample_height(&self, u: f32, v: f32) -> Option<f32> {
        if !u.is_finite() || !v.is_finite() || u < 0.0 || v < 0.0 {
            return None;
        }
        let max_u = ((self.columns - 1) as f64 * self.scale.x as f64) as f32;
        let max_v = ((self.rows - 1) as f64 * self.scale.z as f64) as f32;
        if u > max_u || v > max_v {
            return None;
        }
        let x = if u == max_u {
            (self.columns - 1) as f64
        } else {
            u as f64 / self.scale.x as f64
        };
        let z = if v == max_v {
            (self.rows - 1) as f64
        } else {
            v as f64 / self.scale.z as f64
        };
        let col = (x.floor() as usize).min(self.columns - 2);
        let row = (z.floor() as usize).min(self.rows - 2);
        let (x, z) = (x - col as f64, z - row as f64);
        let i = row * self.columns + col;
        let [a, b, c, d] = [
            self.heights[i],
            self.heights[i + 1],
            self.heights[i + self.columns],
            self.heights[i + self.columns + 1],
        ]
        .map(|h| h as f64);
        let height = if x + z <= 1.0 {
            a * (1.0 - x - z) + b * x + c * z
        } else {
            b * (1.0 - z) + c * (1.0 - x) + d * (x + z - 1.0)
        };
        Some((height * self.scale.y as f64) as f32)
    }
    /// Closest two-sided surface hit, retaining [`TriangleMesh`] ray units,
    /// winding normals, numerical limits and earliest-index exact ties.
    pub fn ray_intersect(&self, ray: &Ray) -> Option<ClosestHit> {
        self.mesh.ray_intersect(ray)
    }
    /// Inclusive surface overlap with a world-space AABB. A box wholly below
    /// the surface misses; terrain has no solid interior. Invalid boxes miss.
    pub fn overlaps_aabb(&self, bounds: &Aabb) -> bool {
        self.mesh.overlaps_aabb(bounds)
    }
}
