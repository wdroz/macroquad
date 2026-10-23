#![cfg(feature = "gltf")]

use macroquad::gltf::{GltfSkinEvaluation, GltfSkinPrimitive};
use macroquad::math::{Mat4, Quat, Vec3};

// Keep the straightforward scalar formulation independent of the specialized
// production loops, including influence order and inverse-transpose normals.
fn reference(
    skin: &GltfSkinPrimitive,
    positions: &[Vec3],
    normals: &[Vec3],
    matrices: &[Mat4],
) -> GltfSkinEvaluation {
    let inverse_transposes = matrices
        .iter()
        .map(|matrix| matrix.inverse().transpose())
        .collect::<Vec<_>>();
    let mut result = GltfSkinEvaluation {
        positions: Vec::new(),
        normals: Vec::new(),
    };
    for index in 0..positions.len() {
        let mut position = Vec3::ZERO;
        let mut normal = Vec3::ZERO;
        for influence in 0..8 {
            let joint = skin.joints[index][influence] as usize;
            let weight = skin.weights[index][influence];
            if weight == 0. {
                continue;
            }
            position += matrices[joint].transform_point3(positions[index]) * weight;
            if !normals.is_empty() {
                normal += inverse_transposes[joint].transform_vector3(normals[index]) * weight;
            }
        }
        result.positions.push(position);
        if !normals.is_empty() {
            result.normals.push(normal.normalize_or_zero());
        }
    }
    result
}

fn assert_bits_equal(actual: &[Vec3], expected: &[Vec3]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            actual.to_array().map(f32::to_bits),
            expected.to_array().map(f32::to_bits)
        );
    }
}

#[test]
fn deformation_matches_scalar_reference_for_one_to_eight_influences() {
    let matrices = (0..11)
        .map(|index| {
            let value = index as f32;
            Mat4::from_scale_rotation_translation(
                Vec3::new(-1. - value * 0.07, 0.4 + value * 0.03, 1.7),
                Quat::from_rotation_y(value * 0.17) * Quat::from_rotation_z(value * 0.11),
                Vec3::new(value * 0.13, -value * 0.2, value * 0.07),
            )
        })
        .collect::<Vec<_>>();
    let positions = (0..128)
        .map(|index| {
            Vec3::new(
                (index % 9) as f32 * 0.31 - 1.,
                (index % 11) as f32 * -0.12,
                (index % 13) as f32 * 0.18,
            )
        })
        .collect::<Vec<_>>();
    let normals = (0..positions.len())
        .map(|index| {
            if index % 5 == 0 {
                Vec3::ZERO
            } else {
                Vec3::new(1., (index % 3) as f32 - 1., -0.7).normalize()
            }
        })
        .collect::<Vec<_>>();
    let skin = GltfSkinPrimitive {
        joints: (0..positions.len())
            .map(|index| std::array::from_fn(|slot| ((index + slot * 3) % matrices.len()) as u16))
            .collect(),
        weights: (0..positions.len())
            .map(|index| {
                let count = index % 8 + 1;
                let mut weights: [f32; 8] = std::array::from_fn(|slot| {
                    // Rotate nonzero influences through both sets, including
                    // leading/interior zeros and all eight nonzero slots.
                    if (slot + index) % 8 < count {
                        (slot + 1) as f32
                    } else {
                        0.
                    }
                });
                let sum = weights.iter().sum::<f32>();
                weights.iter_mut().for_each(|weight| *weight /= sum);
                weights
            })
            .collect(),
    };
    let before = (
        skin.clone(),
        positions.clone(),
        normals.clone(),
        matrices.clone(),
    );
    for source_normals in [&[][..], normals.as_slice()] {
        let expected = reference(&skin, &positions, source_normals, &matrices);
        let actual = skin.deform(&positions, source_normals, &matrices).unwrap();
        assert_bits_equal(&actual.positions, &expected.positions);
        assert_bits_equal(&actual.normals, &expected.normals);
    }
    assert_eq!((skin, positions, normals, matrices), before);
}

#[test]
fn position_only_skinning_still_validates_unused_inputs() {
    let skin = GltfSkinPrimitive {
        joints: vec![[0; 8]],
        weights: vec![[1., 0., 0., 0., 0., 0., 0., 0.]],
    };
    let mut non_affine = Mat4::IDENTITY;
    non_affine.x_axis.w = 0.5;
    for invalid in [
        Mat4::from_scale(Vec3::new(0., 1., 1.)),
        Mat4::from_translation(Vec3::splat(f32::NAN)),
        non_affine,
    ] {
        for normals in [&[][..], &[Vec3::Y][..]] {
            assert!(skin
                .deform(&[Vec3::ZERO], normals, &[Mat4::IDENTITY, invalid])
                .is_err());
        }
        // An empty mesh must not bypass validation of the matrix palette.
        assert!(GltfSkinPrimitive::default()
            .deform(&[], &[], &[invalid])
            .is_err());
    }
    let mut bad_joint = skin.clone();
    bad_joint.joints[0][7] = 1;
    assert!(bad_joint
        .deform(&[Vec3::ZERO], &[], &[Mat4::IDENTITY])
        .is_err());
    for value in [f32::NAN, -0.5, 0.5] {
        let mut bad_weight = skin.clone();
        bad_weight.weights[0][7] = value;
        assert!(bad_weight
            .deform(&[Vec3::ZERO], &[], &[Mat4::IDENTITY])
            .is_err());
    }
    assert_eq!(
        GltfSkinPrimitive::default().deform(&[], &[], &[]).unwrap(),
        GltfSkinEvaluation {
            positions: Vec::new(),
            normals: Vec::new(),
        }
    );
}

#[test]
fn both_skinning_paths_reject_deformation_overflow() {
    let skin = GltfSkinPrimitive {
        joints: vec![[0; 8]],
        weights: vec![[1., 0., 0., 0., 0., 0., 0., 0.]],
    };
    for normals in [&[][..], &[Vec3::Y][..]] {
        let error = skin
            .deform(
                &[Vec3::splat(f32::MAX)],
                normals,
                &[Mat4::from_scale(Vec3::splat(2.))],
            )
            .unwrap_err();
        assert!(error.to_string().contains("deformation overflow"));
    }
    let error = skin
        .deform(
            &[Vec3::ZERO],
            &[Vec3::splat(f32::MAX)],
            &[Mat4::from_scale(Vec3::splat(0.5))],
        )
        .unwrap_err();
    assert!(error.to_string().contains("deformation overflow"));
}

#[test]
fn weight_validation_matches_finite_nonnegative_contract_for_ieee_edge_values() {
    let mut bits = vec![
        0,
        0x8000_0000, // Both signed zeros.
        1,
        0x8000_0001,
        0x007f_ffff,
        0x807f_ffff, // Smallest/largest subnormals.
        0x0080_0000,
        0x8080_0000, // Smallest normal magnitudes.
        0x3f80_0000,
        0xbf80_0000,
        0x7f7f_ffff,
        0xff7f_ffff,
        0x7f80_0000,
        0xff80_0000, // Infinities.
        0x7f80_0001,
        0xff80_0001,
        0x7fc0_0000,
        0xffc0_0000,
        0x7fff_ffff,
        0xffff_ffff, // Multiple quiet/signaling NaN payloads.
    ];
    let mut seed = 0xc012_5eed_u32;
    for _ in 0..256 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        bits.push(seed);
    }
    for value in bits
        .into_iter()
        .map(f32::from_bits)
        .chain([0.0005, 0.9995, 1.0005])
    {
        for slot in 0..8 {
            let mut weights = [1., 0., 0., 0., 0., 0., 0., 0.];
            weights[slot] = value;
            let valid = weights
                .iter()
                .all(|weight| weight.is_finite() && *weight >= 0.)
                && (weights.iter().sum::<f32>() - 1.).abs() <= 0.001;
            let skin = GltfSkinPrimitive {
                joints: vec![[0; 8]],
                weights: vec![weights],
            };
            for normals in [&[][..], &[Vec3::Y][..]] {
                let result = skin.deform(&[Vec3::X], normals, &[Mat4::IDENTITY]);
                assert_eq!(
                    result.is_ok(),
                    valid,
                    "weight bits {:08x}, slot {slot}",
                    value.to_bits()
                );
                if let Err(error) = result {
                    assert_eq!(
                        error.to_string(),
                        "glTF skin deformation has invalid arrays, indices, weights or matrices"
                    );
                }
            }
        }
    }
    // Every value is individually valid, but the ordered sum can overflow.
    let overflow = GltfSkinPrimitive {
        joints: vec![[0; 8]],
        weights: vec![[f32::MAX; 8]],
    };
    assert!(overflow
        .deform(&[Vec3::ZERO], &[], &[Mat4::IDENTITY])
        .is_err());
}
