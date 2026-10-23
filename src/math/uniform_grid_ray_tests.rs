use super::*;
use crate::math::{vec3, Vec3};

// Frozen pre-preparation predicate: this deliberately recomputes each endpoint
// in the per-bound axis loop, independently of the cached envelope under test.
fn legacy_meets_bounds(bounds: &Aabb, ray: &Ray, max_t: f32) -> bool {
    let origin = ray.origin.to_array().map(f64::from);
    let direction = ray.direction.to_array().map(f64::from);
    let max_t = f64::from(max_t);
    let min = bounds.min.to_array();
    let max = bounds.max.to_array();
    let mut enter = 0.0_f64;
    let mut exit = max_t;
    for axis in 0..3 {
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

fn check(ray: &Ray, limit: f32, bounds: &[Aabb]) {
    let prepared = RayBoundsQuery::new(ray, limit);
    for bounds in bounds {
        assert_eq!(
            prepared.meets_bounds(bounds),
            legacy_meets_bounds(bounds, ray, limit),
            "ray {ray:?}, limit {limit:?}, bounds {bounds:?}"
        );
    }
}

#[test]
fn prepared_ray_bounds_match_legacy_at_numeric_edges() {
    let tiny = f32::from_bits(1);
    let mut bounds = [-f32::MAX, -1.0, -tiny, -0.0, 0.0, tiny, 1.0, f32::MAX]
        .into_iter()
        .flat_map(|value| {
            [
                Vec3::splat(value),
                Vec3::X * value,
                Vec3::Y * value,
                Vec3::Z * value,
            ]
        })
        .map(|point| Aabb::from_min_max(point, point))
        .collect::<Vec<_>>();
    bounds.extend([
        Aabb::from_min_max(Vec3::splat(-f32::MAX), Vec3::splat(f32::MAX)),
        Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
        Aabb::from_min_max(vec3(-2.0, 1.0, -1.0), vec3(2.0, 1.0, 1.0)),
    ]);
    let rays = [
        Ray::new(Vec3::ZERO, Vec3::X * tiny),
        Ray::new(Vec3::ZERO, Vec3::splat(-tiny)),
        Ray::new(Vec3::ONE, -Vec3::ONE),
        Ray::new(Vec3::splat(-f32::MAX), Vec3::ONE),
        Ray::new(Vec3::splat(-f32::MAX), Vec3::splat(f32::MAX)),
        Ray::new(Vec3::splat(f32::MAX), Vec3::splat(-tiny)),
        Ray::new(Vec3::ZERO, vec3(-0.0, 0.0, -0.0)),
        Ray::new(Vec3::ONE, Vec3::ZERO),
        Ray::new(vec3(-1.0, 1.0, 1.0), Vec3::X),
        Ray::new(
            vec3(0.5, 0.0, 0.0),
            Vec3::X * f32::from_bits(0.5_f32.to_bits() - 1),
        ),
    ];
    for ray in rays {
        for limit in [
            -0.0,
            0.0,
            tiny,
            1.0,
            f32::from_bits(1.0_f32.to_bits() + 1),
            2.0,
            f32::MAX,
            f32::INFINITY,
        ] {
            check(&ray, limit, &bounds);
        }
    }
}

fn finite(seed: &mut u32) -> f32 {
    loop {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let value = f32::from_bits(*seed);
        if value.is_finite() {
            return value;
        }
    }
}

fn vector(seed: &mut u32) -> Vec3 {
    vec3(finite(seed), finite(seed), finite(seed))
}

#[test]
fn prepared_ray_bounds_match_legacy_for_random_finite_bits() {
    let mut seed = 0x1831_7ab2_u32;
    for index in 0..10_000 {
        let ray = Ray::new(vector(&mut seed), vector(&mut seed));
        let limit = match index % 6 {
            0 => 0.0,
            1 => f32::from_bits(1),
            2 => 1.0,
            3 => f32::MAX,
            4 => f32::INFINITY,
            _ => finite(&mut seed).abs(),
        };
        let a = vector(&mut seed);
        let b = vector(&mut seed);
        let bounds = [
            Aabb::from_min_max(a.min(b), a.max(b)),
            Aabb::from_min_max(a, a),
            Aabb::from_min_max(ray.origin, ray.origin),
            Aabb::from_min_max(a.min(ray.origin), a.max(ray.origin)),
        ];
        check(&ray, limit, &bounds);
    }
}
