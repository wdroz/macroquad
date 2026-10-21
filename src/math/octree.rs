//! Immutable, caller-indexed world-space broadphase.

use std::collections::BTreeMap;

use super::uniform_grid::{ray_meets_bounds, valid_bounds};
use super::{Aabb, Ray};

const MAX_DEPTH: u32 = 16;
const LEAF_SIZE: usize = 8;

#[derive(Clone, Debug)]
struct Node {
    bounds: Aabb,
    entries: Vec<(u32, Aabb)>,
    children: Vec<Node>,
}

impl Node {
    fn build(entries: Vec<(u32, Aabb)>, depth: u32) -> Self {
        let mut bounds = entries[0].1;
        for (_, entry) in &entries {
            bounds.min = bounds.min.min(entry.min);
            bounds.max = bounds.max.max(entry.max);
        }
        let mut node = Self {
            bounds,
            entries: Vec::new(),
            children: Vec::new(),
        };
        if depth == 0 || entries.len() <= LEAF_SIZE {
            node.entries = entries;
            return node;
        }
        // Wide addition avoids overflow for finite corners near f32::MAX.
        let middle: [f32; 3] = std::array::from_fn(|axis| {
            ((f64::from(bounds.min[axis]) + f64::from(bounds.max[axis])) * 0.5) as f32
        });
        let mut buckets: [Vec<(u32, Aabb)>; 8] = std::array::from_fn(|_| Vec::new());
        for (index, entry) in entries {
            let mut octant = 0;
            let mut fits = true;
            for (axis, &split) in middle.iter().enumerate() {
                if entry.max[axis] <= split {
                    // Split-plane points choose the lower child deterministically.
                } else if entry.min[axis] >= split {
                    octant |= 1 << axis;
                } else {
                    fits = false;
                    break;
                }
            }
            if fits {
                buckets[octant].push((index, entry));
            } else {
                node.entries.push((index, entry));
            }
        }
        for bucket in buckets {
            if bucket.is_empty() {
                continue;
            }
            // All entries falling into one unchanged cluster cannot benefit
            // from another identical split (notably coincident point boxes).
            if node.entries.is_empty()
                && node.children.is_empty()
                && bucket
                    .iter()
                    .all(|(_, b)| b.min == bounds.min && b.max == bounds.max)
            {
                node.entries = bucket;
            } else {
                node.children.push(Self::build(bucket, depth - 1));
            }
        }
        node
    }

    fn query(&self, meets: &impl Fn(&Aabb) -> bool, result: &mut Vec<u32>) {
        if !meets(&self.bounds) {
            return;
        }
        result.extend(
            self.entries
                .iter()
                .filter_map(|(i, b)| meets(b).then_some(*i)),
        );
        for child in &self.children {
            child.query(meets, result);
        }
    }
}

/// An immutable octree of world-space [`Aabb`] snapshots, keyed by caller `u32`s.
///
/// Build once for a static scene; construct a new tree when bounds change. The
/// caller retains ownership of the actual objects. Unlike [`super::UniformGrid`],
/// this index has no insert/remove operations. Each box is stored once: boxes
/// crossing split planes stay in their parent. Nodes bound their actual contents,
/// and split each axis at its midpoint without assuming an up axis or cubic world.
/// Leaves target eight entries, but spanning/degenerate clusters may be larger.
/// Depth is capped at 16, including when a larger limit is requested.
///
/// Queries filter stored bounds and return unique ascending indices, not contact
/// results or ray-distance order. Touching counts. Ray tests share the grid's
/// double-precision slab arithmetic: extreme rounding can retain extra candidates;
/// these are broadphase predicates, not exact-arithmetic guarantees. No measured
/// performance advantage over the grid or a linear scan is implied.
///
/// ```
/// use macroquad::prelude::*;
/// let wall = Aabb::from_center_size(Vec3::X * 4.0, Vec3::ONE);
/// let tree = Octree::new([(42, wall)]).unwrap();
/// assert_eq!(tree.query_aabb(&wall), vec![42]);
/// assert_eq!(tree.query_ray(&Ray::new(Vec3::ZERO, Vec3::X * 2.0), 2.0), vec![42]);
/// ```
#[derive(Clone, Debug)]
pub struct Octree {
    root: Option<Node>,
    len: usize,
    max_depth: u32,
}

impl Octree {
    /// Builds with a maximum of 16 subdivisions. See [`Self::with_max_depth`]
    /// for validation and duplicate-index policy.
    pub fn new(entries: impl IntoIterator<Item = (u32, Aabb)>) -> Option<Self> {
        Self::with_max_depth(entries, MAX_DEPTH)
    }

    /// Builds with `max_depth.min(16)` subdivisions; zero creates a linear leaf.
    ///
    /// Every box must have finite corners with `min <= max` on every axis.
    /// Points, lines and planes are valid. Any invalid box rejects the entire
    /// input, even if a later duplicate would replace it. For valid repeated
    /// indices, the last bounds win. Empty input creates an empty tree.
    pub fn with_max_depth(
        entries: impl IntoIterator<Item = (u32, Aabb)>,
        max_depth: u32,
    ) -> Option<Self> {
        let mut unique = BTreeMap::new();
        for (index, bounds) in entries {
            if !valid_bounds(&bounds) {
                return None;
            }
            unique.insert(index, bounds);
        }
        let len = unique.len();
        let max_depth = max_depth.min(MAX_DEPTH);
        let root = if len == 0 {
            None
        } else {
            Some(Node::build(unique.into_iter().collect(), max_depth))
        };
        Some(Self {
            root,
            len,
            max_depth,
        })
    }

    /// Number of distinct caller indices.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether there are no stored entries.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Configured subdivision limit after clamping to 16.
    pub fn max_depth(&self) -> u32 {
        self.max_depth
    }

    /// Union of the stored bounds, or `None` for an empty tree.
    pub fn bounds(&self) -> Option<Aabb> {
        self.root.as_ref().map(|n| n.bounds)
    }

    /// Returns sorted unique indices overlapping `bounds`, including touches.
    /// Invalid (non-finite or inverted) query bounds return an empty vector.
    pub fn query_aabb(&self, bounds: &Aabb) -> Vec<u32> {
        if !valid_bounds(bounds) {
            return Vec::new();
        }
        self.query(|stored| stored.overlaps(bounds))
    }

    /// Returns sorted unique indices meeting `origin + direction * t` for
    /// `0 <= t <= max_t`. A unit direction makes `t` a world distance.
    /// Endpoints/tangencies count; zero direction or zero limit tests the origin.
    /// Finite ray components are required; the limit may be positive infinity.
    /// NaN/negative limits and invalid rays return an empty vector. Nonzero
    /// subnormal directions are retained; no parallel-direction epsilon is used.
    pub fn query_ray(&self, ray: &Ray, max_t: f32) -> Vec<u32> {
        if !ray.origin.is_finite() || !ray.direction.is_finite() || max_t.is_nan() || max_t < 0.0 {
            return Vec::new();
        }
        self.query(|bounds| {
            ray_meets_bounds(
                bounds,
                ray.origin.to_array().map(f64::from),
                ray.direction.to_array().map(f64::from),
                f64::from(max_t),
            )
        })
    }

    fn query(&self, meets: impl Fn(&Aabb) -> bool) -> Vec<u32> {
        let mut result = Vec::new();
        if let Some(root) = &self.root {
            root.query(&meets, &mut result);
        }
        result.sort_unstable();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::Vec3;
    use super::*;

    #[test]
    fn degenerate_cluster_stops_and_depth_is_bounded() {
        let point = Aabb::from_min_max(Vec3::ZERO, Vec3::ZERO);
        let tree = Octree::with_max_depth((0..10000).map(|i| (i, point)), u32::MAX).unwrap();
        assert_eq!(tree.max_depth(), 16);
        assert!(tree.root.as_ref().unwrap().children.is_empty());
        assert_eq!(tree.query_aabb(&point).len(), 10000);
        fn depth(n: &Node) -> u32 {
            n.children.iter().map(depth).max().map_or(0, |d| d + 1)
        }
        let boxes: Vec<_> = (0..128)
            .map(|i| {
                let p = Vec3::splat(2.0_f32.powi(i - 64));
                (i as u32, Aabb::from_min_max(p, p))
            })
            .collect();
        for limit in [0, 1, 2, 16, u32::MAX] {
            let tree = Octree::with_max_depth(boxes.clone(), limit).unwrap();
            let actual = depth(tree.root.as_ref().unwrap());
            assert!(actual <= limit.min(16));
            assert_eq!(actual == 0, limit == 0);
            assert_eq!(tree.query_aabb(&tree.bounds().unwrap()).len(), 128);
        }
    }
}
