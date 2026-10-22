//! Asset-free CPU example: a whole sphere can hit where its center ray misses.
//! Run with `cargo run --example sphere_sweep`; no window is required.
use macroquad::prelude::*;

fn main() {
    let rock = Sphere::new(Vec3::ZERO, 1.0);
    let body = Sphere::new(vec3(-3.0, 1.2, 0.0), 0.5);
    let delta = Vec3::X * 6.0;
    assert!(rock.ray_intersect(body.center, delta).is_none());
    let hit = body.sweep_sphere(delta, &rock).unwrap();
    let impact_center = body.center + delta * hit.t;
    println!(
        "Sphere impact: fraction={}, center={impact_center:?}",
        hit.t
    );
    println!(
        "Static surface point={:?}, outward normal={:?}",
        hit.point, hit.normal
    );
    assert!((hit.t - 0.35).abs() < 1e-5);
    assert!(((impact_center - hit.point).length() - body.radius).abs() < 1e-5);

    // One query reports one solid's impact. The caller chooses how to move
    // and which solids to test; this does not integrate velocity or slide.
    let permitted_translation = delta * hit.t;
    assert!((body.offset(permitted_translation).center - impact_center).length() < 1e-5);
    let resting = Sphere::new(vec3(0.0, 1.5, 0.0), body.radius);
    assert!(resting.sweep_sphere(Vec3::X, &rock).is_none());
    assert_eq!(resting.sweep_sphere(-Vec3::Y, &rock).unwrap().t, 0.0);
}
