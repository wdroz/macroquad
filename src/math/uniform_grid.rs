//! A caller-indexed, world-space uniform grid.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::{Aabb, Ray};

// Bound cell expansion independently of world size and the chosen cell size.
const MAX_CELLS: u64 = 4096;
type Cell = [i32; 3];

#[derive(Clone, Copy, Debug)]
struct CellRange {
    min: Cell,
    max: Cell,
}

impl CellRange {
    fn visit(self, mut visit: impl FnMut(Cell)) {
        for x in self.min[0]..=self.max[0] {
            for y in self.min[1]..=self.max[1] {
                for z in self.min[2]..=self.max[2] {
                    visit([x, y, z]);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    bounds: Aabb,
    cells: Option<CellRange>,
}

/// A uniform-grid broadphase storing world-space [`Aabb`] snapshots by caller index.
///
/// The caller owns its objects and chooses their `u32` indices. Inserting an
/// existing index replaces its bounds; moving an object requires another insert
/// or a [`rebuild`](Self::rebuild). A box occupies every cell from the floor of
/// its minimum through the floor of its maximum, including the maximum cell.
/// Face, edge and point touches therefore remain queryable at cell boundaries,
/// including negative coordinates. No up axis is assumed.
///
/// Queries return unique indices in ascending numeric order, filtered against
/// the stored bounds. They are broadphase candidates for the caller's actual
/// geometry, not contact results. Large bounds or very small cell sizes never
/// expand into an unbounded number of cells: entries spanning more than 4096
/// cells, or outside signed 32-bit cell coordinates, use a fallback list checked
/// by every query. Large or unbounded queries scan the stored bounds. These
/// fallbacks preserve results but may make a query linear in the object count.
///
/// # Example
///
/// ```
/// use macroquad::prelude::*;
///
/// let mut grid = UniformGrid::new(2.0).unwrap();
/// let wall = Aabb::from_min_max(vec3(1.0, -1.0, -1.0), vec3(5.0, 1.0, 1.0));
/// assert!(grid.insert(42, wall));
/// let point = Aabb::from_min_max(vec3(2.0, 0.0, 0.0), vec3(2.0, 0.0, 0.0));
/// assert_eq!(grid.query_aabb(&point), vec![42]);
/// assert_eq!(grid.query_ray(&Ray::new(Vec3::ZERO, Vec3::X * 2.0), 0.5), vec![42]);
/// ```
#[derive(Clone, Debug)]
pub struct UniformGrid {
    cell_size: f32,
    entries: BTreeMap<u32, Entry>,
    cells: HashMap<Cell, Vec<u32>>,
    fallback: BTreeSet<u32>,
}

impl UniformGrid {
    /// Creates an empty grid, or [`None`] unless `cell_size` is finite and positive.
    ///
    /// Cell size is measured in the same world-space units as the stored bounds.
    /// Any positive finite value, including a subnormal value, is accepted.
    pub fn new(cell_size: f32) -> Option<Self> {
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return None;
        }
        Some(Self {
            cell_size,
            entries: BTreeMap::new(),
            cells: HashMap::new(),
            fallback: BTreeSet::new(),
        })
    }

    /// The world-space cell size supplied to [`Self::new`].
    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    /// Number of distinct caller indices currently stored.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no caller indices are stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Inserts or replaces the bounds for `index`, returning whether they are valid.
    ///
    /// Both corners must be finite and `min <= max` on every axis. Degenerate
    /// boxes (planes, lines and points) are valid. Invalid bounds return `false`
    /// and leave any previous entry unchanged; corners are not normalized
    /// automatically. Very large valid bounds use the fallback described above.
    ///
    /// The grid stores a copy: refresh the same index after moving its object.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let mut grid = UniformGrid::new(2.0).unwrap();
    /// let old_bounds = Aabb::from_center_size(Vec3::ZERO, Vec3::ONE);
    /// assert!(grid.insert(7, old_bounds));
    /// let new_bounds = Aabb::from_center_size(Vec3::X * 5.0, Vec3::ONE);
    /// assert!(grid.insert(7, new_bounds));
    /// assert_eq!(grid.len(), 1);
    /// assert!(grid.query_aabb(&old_bounds).is_empty());
    /// assert_eq!(grid.query_aabb(&new_bounds), vec![7]);
    /// ```
    pub fn insert(&mut self, index: u32, bounds: Aabb) -> bool {
        if !valid_bounds(&bounds) {
            return false;
        }
        let cells = self.bounds_cells(&bounds);
        if let Some(previous) = self.entries.remove(&index) {
            if let Some(range) = previous.cells {
                range.visit(|cell| {
                    if let Some(indices) = self.cells.get_mut(&cell) {
                        indices.retain(|&old| old != index);
                        if indices.is_empty() {
                            self.cells.remove(&cell);
                        }
                    }
                });
            } else {
                self.fallback.remove(&index);
            }
        }
        if let Some(range) = cells {
            range.visit(|cell| self.cells.entry(cell).or_default().push(index));
        } else {
            self.fallback.insert(index);
        }
        self.entries.insert(index, Entry { bounds, cells });
        true
    }

    /// Removes every entry, retaining the configured cell size.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.cells.clear();
        self.fallback.clear();
    }

    /// Clears the grid and inserts the given `(caller_index, bounds)` snapshots.
    ///
    /// Returns the final number of distinct stored indices. Invalid bounds are
    /// skipped; for repeated indices, the last valid bounds win. An empty
    /// iterator therefore clears the grid. The cell size stays unchanged.
    ///
    /// ```
    /// use macroquad::math::{Aabb, UniformGrid, Vec3};
    /// let mut grid = UniformGrid::new(1.0).unwrap();
    /// let bounds = Aabb::from_center_size(Vec3::ZERO, Vec3::ONE);
    /// assert_eq!(grid.rebuild([(7, bounds), (7, bounds)]), 1);
    /// grid.clear();
    /// assert!(grid.is_empty());
    /// ```
    pub fn rebuild(&mut self, entries: impl IntoIterator<Item = (u32, Aabb)>) -> usize {
        self.clear();
        for (index, bounds) in entries {
            self.insert(index, bounds);
        }
        self.len()
    }

    /// Returns indices whose stored bounds overlap `bounds`, including touches.
    ///
    /// Results are unique and sorted by caller index. Non-finite or inverted
    /// bounds return an empty vector, matching [`Self::insert`]'s validation.
    pub fn query_aabb(&self, bounds: &Aabb) -> Vec<u32> {
        if !valid_bounds(bounds) {
            return Vec::new();
        }
        self.query(self.bounds_cells(bounds), |entry| entry.overlaps(bounds))
    }

    /// Returns indices whose stored bounds meet the ray at `0 <= t <= max_t`.
    ///
    /// The parameterization is `ray.origin + ray.direction * t`; `max_t` is
    /// world distance only for a unit direction. Endpoints and tangencies count.
    /// A zero direction, or `max_t == 0`, tests the origin point. Results are
    /// unique and sorted by caller index, **not** by distance along the ray.
    /// Run a shape query on the returned candidates if hit positions or normals
    /// are needed. Bounds are tested with double-precision slabs and no parallel
    /// direction epsilon, so nonzero subnormal directions are not discarded.
    /// Rounding at extreme coordinate scales can retain extra candidates; these
    /// are broadphase results, not exact-arithmetic intersection guarantees.
    ///
    /// Origin and direction must be finite. `max_t` may be positive infinity
    /// for the full forward ray; NaN or negative limits return an empty vector.
    /// Infinity is accepted only for the limit, not for a ray component. Finite
    /// queries use cells in the segment's bounding box; a large segment or full
    /// ray scans all stored bounds instead of walking unbounded empty cells.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let mut grid = UniformGrid::new(1.0).unwrap();
    /// grid.insert(20, Aabb::from_center_size(Vec3::X * 2.0, Vec3::ONE));
    /// grid.insert(3, Aabb::from_center_size(Vec3::X * 4.0, Vec3::ONE));
    /// let ray = Ray::new(Vec3::ZERO, Vec3::X * 2.0);
    /// assert_eq!(grid.query_ray(&ray, 1.0), vec![20]); // Ends at world X = 2.
    /// assert_eq!(grid.query_ray(&ray, 2.0), vec![3, 20]); // Index order, not distance.
    /// ```
    pub fn query_ray(&self, ray: &Ray, max_t: f32) -> Vec<u32> {
        if !ray.origin.is_finite() || !ray.direction.is_finite() || max_t.is_nan() || max_t < 0.0 {
            return Vec::new();
        }
        let origin = ray.origin.to_array().map(f64::from);
        let direction = ray.direction.to_array().map(f64::from);
        let range = if direction == [0.0; 3] || max_t == 0.0 {
            self.cell_range(origin, origin)
        } else if max_t.is_finite() {
            let end = std::array::from_fn::<_, 3, _>(|axis| {
                origin[axis] + direction[axis] * f64::from(max_t)
            });
            // f32 products fit exactly in f64; expand the rounded sum outward
            // once before indexing, without narrowing an endpoint back to f32.
            let min = std::array::from_fn(|axis| next_down(origin[axis].min(end[axis])));
            let max = std::array::from_fn(|axis| next_up(origin[axis].max(end[axis])));
            self.cell_range(min, max)
        } else {
            None
        };
        self.query(range, |bounds| {
            ray_meets_bounds(bounds, origin, direction, f64::from(max_t))
        })
    }

    fn query(&self, range: Option<CellRange>, intersects: impl Fn(&Aabb) -> bool) -> Vec<u32> {
        if let Some(range) = range {
            let mut candidates = self.fallback.clone();
            range.visit(|cell| {
                if let Some(indices) = self.cells.get(&cell) {
                    candidates.extend(indices);
                }
            });
            candidates
                .into_iter()
                .filter(|index| intersects(&self.entries[index].bounds))
                .collect()
        } else {
            self.entries
                .iter()
                .filter_map(|(&index, entry)| intersects(&entry.bounds).then_some(index))
                .collect()
        }
    }

    fn bounds_cells(&self, bounds: &Aabb) -> Option<CellRange> {
        self.cell_range(
            bounds.min.to_array().map(f64::from),
            bounds.max.to_array().map(f64::from),
        )
    }

    fn cell_range(&self, min: [f64; 3], max: [f64; 3]) -> Option<CellRange> {
        let mut range = CellRange {
            min: [0; 3],
            max: [0; 3],
        };
        let mut count = 1_u64;
        for axis in 0..3 {
            let low = (min[axis] / f64::from(self.cell_size)).floor();
            let high = (max[axis] / f64::from(self.cell_size)).floor();
            if low < f64::from(i32::MIN) || high > f64::from(i32::MAX) {
                return None;
            }
            range.min[axis] = low as i32;
            range.max[axis] = high as i32;
            let width = (i64::from(range.max[axis]) - i64::from(range.min[axis]) + 1) as u64;
            count = count.checked_mul(width)?;
            if count > MAX_CELLS {
                return None;
            }
        }
        Some(range)
    }
}

fn valid_bounds(bounds: &Aabb) -> bool {
    bounds.min.is_finite() && bounds.max.is_finite() && bounds.min.cmple(bounds.max).all()
}

fn ray_meets_bounds(bounds: &Aabb, origin: [f64; 3], direction: [f64; 3], max_t: f64) -> bool {
    let min = bounds.min.to_array();
    let max = bounds.max.to_array();
    let mut enter = 0.0_f64;
    let mut exit = max_t;
    for axis in 0..3 {
        // A slab's subtraction may erase a tiny endpoint separation when the
        // origin is very far away. Reject bounds outside the finite segment's
        // coordinate envelope before comparing the rounded parametric times.
        if max_t.is_finite() {
            let end = origin[axis] + direction[axis] * max_t;
            if f64::from(max[axis]) < origin[axis].min(end)
                || f64::from(min[axis]) > origin[axis].max(end)
            {
                return false;
            }
        }
        if direction[axis] == 0.0 {
            if origin[axis] < f64::from(min[axis]) || origin[axis] > f64::from(max[axis]) {
                return false;
            }
        } else {
            let a = (f64::from(min[axis]) - origin[axis]) / direction[axis];
            let b = (f64::from(max[axis]) - origin[axis]) / direction[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return false;
            }
        }
    }
    true
}

// Finite inputs only. Spell these out to avoid raising the crate's Rust version
// requirement solely for f64::next_up / next_down.
fn next_up(value: f64) -> f64 {
    if value == 0.0 {
        f64::from_bits(1)
    } else if value > 0.0 {
        f64::from_bits(value.to_bits() + 1)
    } else {
        f64::from_bits(value.to_bits() - 1)
    }
}

fn next_down(value: f64) -> f64 {
    -next_up(-value)
}
