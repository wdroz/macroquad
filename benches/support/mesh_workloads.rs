//! Surface overlap workloads: remote rejection, local queries and tiny controls.

use super::*;
use macroquad::math::{Triangle, TriangleMesh};

struct MeshFixture {
    mesh: TriangleMesh,
    boxes: Vec<Aabb>,
    spheres: Vec<Sphere>,
    fingerprint: Fingerprint,
}

fn tile_triangle(index: usize) -> Triangle {
    let column = index % 64;
    let row = index / 64;
    let corner = vec3(
        column as f32 * 2.0 - 64.0,
        row as f32 * 2.0 - 64.0,
        ((row + column) % 5) as f32 * 0.25,
    );
    Triangle::new(corner, corner + Vec3::X, corner + Vec3::Y)
}

fn fixture(kind: usize) -> MeshFixture {
    let triangles = if kind == 2 {
        vec![Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y)]
    } else {
        (0..4096)
            .map(|index| {
                // An odd multiplier permutes the power-of-two tile count.
                // This control removes the ordered mesh's spatial locality.
                tile_triangle(if kind == 3 {
                    index * 1091 % 4096
                } else {
                    index
                })
            })
            .collect::<Vec<_>>()
    };
    let mesh = TriangleMesh::new(triangles).unwrap();
    let union = mesh.aabb().unwrap();
    let mut fingerprint = Fingerprint::new();
    fingerprint.bytes(&(mesh.triangles().len() as u64).to_le_bytes());
    for triangle in mesh.triangles() {
        fingerprint.vec3(triangle.a);
        fingerprint.vec3(triangle.b);
        fingerprint.vec3(triangle.c);
    }
    let mut boxes = Vec::new();
    let mut spheres = Vec::new();
    let mut expected_hits = 0;
    for index in 0..32 {
        let (center, radius, hit) = match kind {
            0 => {
                // Disjoint queries on all six sides of the complete surface.
                let mut center = union.center();
                let axis = index % 3;
                center[axis] = if index / 3 % 2 == 0 {
                    union.min[axis] - 3.0
                } else {
                    union.max[axis] + 3.0
                };
                (center, 0.125, false)
            }
            1 | 3 => {
                // Interior tiles cover early and late triangle indices. Half
                // touch faces; half lie in gaps but still inside the union.
                let column = 1 + index * 13 % 62;
                let row = 1 + index * 7 % 62;
                let triangle = tile_triangle(row * 64 + column);
                let hit = index % 2 == 0;
                let offset = if hit { 0.25 } else { 1.375 };
                (triangle.a + vec3(offset, offset, 0.0), 0.125, hit)
            }
            _ => (
                vec3(0.125 + (index % 5) as f32 * 0.0625, 0.25, 0.0),
                0.0625,
                true,
            ),
        };
        let bounds = Aabb::from_center_size(center, Vec3::splat(radius * 2.0));
        let sphere = Sphere::new(center, radius);
        if kind == 1 || kind == 3 {
            assert!(union.contains(center));
        }
        // Expectations follow the fixture's separated tiles and face points,
        // independently of the optimized mesh broadphase.
        assert_eq!(mesh.overlaps_aabb(&bounds), hit);
        assert_eq!(mesh.overlaps_sphere(&sphere), hit);
        fingerprint.aabb(&bounds);
        fingerprint.vec3(sphere.center);
        fingerprint.f32(sphere.radius);
        fingerprint.bytes(&[u8::from(hit)]);
        boxes.push(bounds);
        spheres.push(sphere);
        expected_hits += usize::from(hit);
    }
    assert_eq!(expected_hits, [0, 16, 32, 16][kind]);
    MeshFixture {
        mesh,
        boxes,
        spheres,
        fingerprint,
    }
}

pub(super) fn cases(settings: &Settings) -> usize {
    let mut selected = 0;
    for kind in 0..4 {
        let fixture = fixture(kind);
        selected += run(
            settings,
            [
                "mesh.external_aabb",
                "mesh.local_aabb",
                "mesh.tiny_aabb",
                "mesh.interleaved_aabb",
            ][kind],
            [
                "tiles4096-external32-aabb-v1",
                "tiles4096-local32-hit16-aabb-v1",
                "triangle1-hit32-aabb-v1",
                "tiles4096-interleaved-local32-hit16-aabb-v1",
            ][kind],
            fixture.fingerprint,
            fixture.boxes.len() as u64,
            || {
                let mut digest = Digest::default();
                for bounds in black_box(&fixture.boxes) {
                    let hit = black_box(fixture.mesh.overlaps_aabb(black_box(bounds)));
                    digest.hits += u64::from(hit);
                    digest.value(u64::from(hit));
                }
                digest
            },
        ) as usize;
        selected += run(
            settings,
            [
                "mesh.external_sphere",
                "mesh.local_sphere",
                "mesh.tiny_sphere",
                "mesh.interleaved_sphere",
            ][kind],
            [
                "tiles4096-external32-sphere-v1",
                "tiles4096-local32-hit16-sphere-v1",
                "triangle1-hit32-sphere-v1",
                "tiles4096-interleaved-local32-hit16-sphere-v1",
            ][kind],
            fixture.fingerprint,
            fixture.spheres.len() as u64,
            || {
                let mut digest = Digest::default();
                for sphere in black_box(&fixture.spheres) {
                    let hit = black_box(fixture.mesh.overlaps_sphere(black_box(sphere)));
                    digest.hits += u64::from(hit);
                    digest.value(u64::from(hit));
                }
                digest
            },
        ) as usize;
    }
    let triangles = (0..4096).map(tile_triangle).collect::<Vec<_>>();
    let mut fingerprint = Fingerprint::new();
    fingerprint.bytes(&(triangles.len() as u64).to_le_bytes());
    for triangle in &triangles {
        fingerprint.vec3(triangle.a);
        fingerprint.vec3(triangle.b);
        fingerprint.vec3(triangle.c);
    }
    selected += run(
        settings,
        "mesh.construct",
        "tiles4096-construct-owned-input-clone-v1",
        fingerprint,
        1,
        || {
            // Construction takes ownership: include the caller's input clone
            // explicitly and consistently in both baseline and candidate.
            let mesh = black_box(TriangleMesh::new(black_box(&triangles).clone()).unwrap());
            let mut digest = Digest {
                hits: mesh.triangles().len() as u64,
                ..Default::default()
            };
            digest.value(mesh.triangles().len() as u64);
            let bounds = mesh.aabb().unwrap();
            digest.vec3(bounds.min);
            digest.vec3(bounds.max);
            digest
        },
    ) as usize;
    selected
}
