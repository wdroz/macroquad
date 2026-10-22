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

    // A finite rounded segment adds a barrel and two endpoint caps. The
    // same delta convention works with any caller-defined up direction.
    let post = Capsule::new(vec3(0.0, -2.0, 0.0), vec3(0.0, 2.0, 0.0), 0.25);
    let shot = Sphere::new(vec3(-3.0, 0.0, 0.0), 0.5);
    let barrel_hit = shot.sweep_capsule(delta, &post).unwrap();
    assert!((barrel_hit.t - 0.375).abs() < 1e-6);
    assert_eq!(barrel_hit.normal, -Vec3::X);
    assert_eq!(barrel_hit.point, -Vec3::X * post.radius);
    let above = Sphere::new(vec3(-3.0, 2.6, 0.0), 0.5);
    assert!(post.ray_intersect(above.center, delta).is_none());
    let cap_hit = above.sweep_capsule(delta, &post).unwrap();
    assert!((cap_hit.t - 0.425).abs() < 1e-6);
    println!("Capsule barrel={barrel_hit:?}, cap={cap_hit:?}");
    let horizontal = Capsule::new(-Vec3::X * 2.0, Vec3::X * 2.0, 0.25);
    let z_shot = Sphere::new(-Vec3::Z * 3.0, 0.5);
    let hit = z_shot.sweep_capsule(Vec3::Z * 6.0, &horizontal).unwrap();
    assert!((hit.t - barrel_hit.t).abs() < 1e-6);
    assert_eq!(hit.normal, -Vec3::Z);
}
