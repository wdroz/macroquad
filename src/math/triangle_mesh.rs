//! Immutable triangle surfaces with cached world-space bounds.

use glam::{DVec3, Vec3};

use super::{Aabb, ClosestHit, Ray, Sphere, Triangle};

/// A caller-owned triangle surface, with no assumed up axis or solid interior.
///
/// Triangles retain their input order, including duplicates and zero-area faces.
/// Bounds are cached privately so they cannot become stale. To edit geometry,
/// construct a replacement mesh. Queries scan linearly; there is no BVH.
/// Overlaps include touching edges/vertices and degenerate segments/points.
/// A sphere or box wholly inside a closed mesh without touching its surface
/// does **not** overlap. Ray queries follow [`Triangle::ray_intersect`], including
/// its winding normal, two-sided hits, numeric limits and zero-area misses.
///
/// ```
/// use macroquad::prelude::*;
/// let mesh = TriangleMesh::new(vec![Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y)]).unwrap();
/// let hit = mesh.ray_intersect(&Ray::new(vec3(0.25, 0.25, 2.0), -Vec3::Z)).unwrap();
/// assert_eq!(hit.index, 0);
/// assert_eq!(hit.hit.t, 2.0);
/// assert!(mesh.overlaps_sphere(&Sphere::new(Vec3::ZERO, 0.1)));
/// ```
#[derive(Clone, Debug, Default)]
pub struct TriangleMesh {
    triangles: Vec<Triangle>,
    triangle_bounds: Vec<Aabb>,
    bounds: Option<Aabb>,
}

impl TriangleMesh {
    /// Takes ownership of finite world-space triangles; returns [`None`] if any
    /// corner is non-finite. An empty mesh is valid and all its queries miss.
    pub fn new(triangles: Vec<Triangle>) -> Option<Self> {
        if triangles
            .iter()
            .any(|t| !t.a.is_finite() || !t.b.is_finite() || !t.c.is_finite())
        {
            return None;
        }
        let triangle_bounds: Vec<_> = triangles.iter().map(|t| t.aabb()).collect();
        let bounds = triangle_bounds.iter().copied().reduce(|a, b| Aabb {
            min: a.min.min(b.min),
            max: a.max.max(b.max),
        });
        Some(Self {
            triangles,
            triangle_bounds,
            bounds,
        })
    }

    /// Original triangles in stable index order, without mutable access.
    pub fn triangles(&self) -> &[Triangle] {
        &self.triangles
    }

    /// Cached tight bounds in the same order as [`Self::triangles`].
    pub fn triangle_bounds(&self) -> &[Aabb] {
        &self.triangle_bounds
    }

    /// Tight bounds of all corners, or [`None`] for an empty mesh.
    pub fn aabb(&self) -> Option<Aabb> {
        self.bounds
    }

    /// Closest forward ray hit and original triangle index. Exact ties retain
    /// the first index. Direction need not be normalized; `t` uses ray units.
    /// Non-finite rays miss. Origins on a face can hit at zero, even with a
    /// zero direction, following [`Triangle::ray_intersect`].
    pub fn ray_intersect(&self, ray: &Ray) -> Option<ClosestHit> {
        // Use the established triangle query directly: a separate f32 slab
        // rejection could discard a face hit at a rounded boundary.
        ray.closest_triangle(&self.triangles)
    }

    /// Whether a finite, non-inverted box touches the triangle surface.
    /// Invalid boxes miss. Uses cached bounds followed by a triangle/box
    /// separating-axis test, including all nine edge/box-axis cross products.
    pub fn overlaps_aabb(&self, bounds: &Aabb) -> bool {
        if !bounds.min.is_finite() || !bounds.max.is_finite() || !bounds.min.cmple(bounds.max).all()
        {
            return false;
        }
        self.triangles
            .iter()
            .zip(&self.triangle_bounds)
            .any(|(&t, b)| b.overlaps(bounds) && triangle_box(t, bounds))
    }

    /// Whether a finite, non-negative-radius sphere touches the triangle
    /// surface. Invalid spheres miss. Closest surface points and squared
    /// distances use f64 intermediates, including degenerate edges/vertices.
    /// Interior edge distances use perpendicular distance rather than a
    /// reconstructed closest point, preserving small offsets on long edges.
    /// These are floating-point tests, not exact predicates; extreme aspect
    /// ratios or nearly collinear faces can still lose precision.
    pub fn overlaps_sphere(&self, sphere: &Sphere) -> bool {
        if !sphere.center.is_finite() || !sphere.radius.is_finite() || sphere.radius < 0.0 {
            return false;
        }
        let p = wide(sphere.center);
        let r2 = (sphere.radius as f64).powi(2);
        self.triangles
            .iter()
            .zip(&self.triangle_bounds)
            .any(|(&t, b)| {
                let closest_bound = p.clamp(wide(b.min), wide(b.max));
                p.distance_squared(closest_bound) <= r2 && triangle_distance_squared(t, p) <= r2
            })
    }
}

fn wide(v: Vec3) -> DVec3 {
    DVec3::new(v.x as f64, v.y as f64, v.z as f64)
}

fn triangle_box(t: Triangle, bounds: &Aabb) -> bool {
    let lo = wide(bounds.min);
    let hi = wide(bounds.max);
    let center = (lo + hi) * 0.5;
    let extent = (hi - lo) * 0.5;
    let vertices = [wide(t.a) - center, wide(t.b) - center, wide(t.c) - center];
    let edges = [
        vertices[1] - vertices[0],
        vertices[2] - vertices[1],
        vertices[0] - vertices[2],
    ];
    let separated = |axis: DVec3| {
        let projections = vertices.map(|v| v.dot(axis));
        let min = projections[0].min(projections[1]).min(projections[2]);
        let max = projections[0].max(projections[1]).max(projections[2]);
        let radius = extent.dot(axis.abs());
        min > radius || max < -radius
    };
    if separated(edges[0].cross(edges[1])) {
        return false;
    }
    for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
        if separated(axis) || edges.iter().any(|edge| separated(edge.cross(axis))) {
            return false;
        }
    }
    true
}

fn triangle_distance_squared(t: Triangle, p: DVec3) -> f64 {
    let [a, b, c] = [wide(t.a), wide(t.b), wide(t.c)];
    let ab = b - a;
    let ac = c - a;
    let n = ab.cross(ac);
    let n2 = n.length_squared();
    if n2 > 0.0 {
        let ap = p - a;
        // Cross-product barycentrics avoid cancellation in the Gram determinant.
        let u = ap.cross(ac).dot(n) / n2;
        let v = ab.cross(ap).dot(n) / n2;
        if u >= 0.0 && v >= 0.0 && u + v <= 1.0 {
            return ap.dot(n).powi(2) / n2;
        }
    }
    [(a, b), (b, c), (c, a)]
        .into_iter()
        .map(|(start, end)| {
            let edge = end - start;
            let len2 = edge.length_squared();
            if len2 == 0.0 {
                return p.distance_squared(start);
            }
            let projection = (p - start).dot(edge);
            if projection <= 0.0 {
                return p.distance_squared(start);
            }
            if projection >= len2 {
                return p.distance_squared(end);
            }
            // Reconstructing start + edge * t can erase a small closest
            // point on an edge spanning large opposite coordinates. Measure
            // the perpendicular distance instead, using the nearest of the
            // endpoints/midpoint to reduce cancellation in the offset.
            let anchor = [start, end, (start + end) * 0.5]
                .into_iter()
                .min_by(|a, b| p.distance_squared(*a).total_cmp(&p.distance_squared(*b)))
                .unwrap();
            (p - anchor).cross(edge).length_squared() / len2
        })
        .fold(f64::INFINITY, f64::min)
}
