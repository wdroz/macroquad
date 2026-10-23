//! Native and WebGL pixel test of one imported child-transform/weight clip.

#[path = "gltf_animation_asset.rs"]
mod asset;

use macroquad::{prelude::*, telemetry};

fn read(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn center(image: &Image) -> Vec2 {
    let mut sum = Vec2::ZERO;
    let mut count = 0;
    for (index, pixel) in image.bytes.chunks_exact(4).enumerate() {
        if pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240 {
            sum += vec2(
                (index % image.width as usize) as f32 + 0.5,
                (index / image.width as usize) as f32 + 0.5,
            );
            count += 1;
        }
    }
    assert!(count > 30, "animated quad must cover visible pixels");
    sum / count as f32
}

fn draw(morph: &GltfMorphInstance, pose: &GltfHierarchyPose) {
    for part in &morph.model().parts {
        let node = pose.node_pose(pose.handle(part.node).unwrap()).unwrap();
        part.draw_posed(&node).unwrap();
    }
}

fn uploads(expected: u64) {
    assert_eq!(
        telemetry::renderer_diagnostics()
            .counters
            .retained_buffer_uploads,
        expected
    );
}

pub async fn run() {
    let bytes = asset::glb();
    let clip = GltfAnimationData::from_bytes(&bytes)
        .unwrap()
        .clips
        .remove(0);
    let source = GltfMorphModel::from_bytes(&bytes).unwrap();
    let mut first = source.instantiate();
    let second = source.instantiate();
    let mut pose = source.pose().hierarchy().unwrap();
    let rest_pose = pose.clone();
    let mut player = GltfAnimationPlayer::new(clip.clone());
    let other = player.clone();
    let target = render_target_ex(
        64,
        64,
        RenderTargetParams {
            depth: true,
            ..Default::default()
        },
    );
    let camera = Camera3D {
        position: vec3(0., 0., 4.),
        target: Vec3::ZERO,
        up: Vec3::Y,
        projection: Projection::Orthographics,
        fovy: 2.,
        aspect: Some(1.),
        z_near: 0.1,
        z_far: 10.,
        render_target: Some(target.clone()),
        ..Default::default()
    };
    set_camera(&camera);
    telemetry::set_renderer_diagnostics_enabled(true);
    telemetry::reset_renderer_diagnostics();
    assert!(!player
        .sample()
        .unwrap()
        .apply(&mut pose, Some(&mut first))
        .unwrap());
    uploads(0);
    clear_background(BLACK);
    draw(&first, &pose); // Queued neutral draw retains pose and geometry.
    assert!(player
        .seek(2.)
        .unwrap()
        .apply(&mut pose, Some(&mut first))
        .unwrap());
    uploads(2);
    let queued = center(&read(&target));
    assert!((queued.x - 16.).abs() < 1.);
    clear_background(BLACK);
    draw(&first, &pose);
    let animated = center(&read(&target));
    assert!((animated.x - 41.6).abs() < 1., "{animated:?}");
    assert!(
        ((animated.y - queued.y).abs() - 6.4).abs() < 1.,
        "{animated:?} vs {queued:?}"
    );
    assert_eq!(first.weights(1).unwrap(), [1.]);
    assert_eq!(second.weights(1).unwrap(), [0.]);
    assert_eq!(other.time(), 0.);
    let part = &first.model().parts[0];
    let node = pose.node_pose(pose.handle(1).unwrap()).unwrap();
    assert!(part
        .posed_bounds(&node, Mat4::IDENTITY)
        .unwrap()
        .center()
        .abs_diff_eq(vec3(0.3, 0.4, 0.), 1e-5));
    telemetry::reset_renderer_diagnostics();
    player.set_paused(true);
    assert!(!player
        .advance(5.)
        .unwrap()
        .apply(&mut pose, Some(&mut first))
        .unwrap());
    uploads(0);
    // Invalid combined edits must not commit the valid transform prefix.
    let world = pose.world(pose.handle(1).unwrap()).unwrap();
    let mut bad = clip.sample(0.).unwrap();
    bad.weights[0].1[0] = f32::NAN;
    assert!(bad.apply(&mut pose, Some(&mut first)).is_err());
    assert_eq!(pose.world(pose.handle(1).unwrap()).unwrap(), world);
    assert_eq!(first.weights(1).unwrap(), [1.]);
    uploads(0);
    clear_background(BLACK);
    draw(&second, &rest_pose);
    assert!(center(&read(&target)).abs_diff_eq(queued, 0.01));
    player.reset();
    assert!(player
        .sample()
        .unwrap()
        .apply(&mut pose, Some(&mut first))
        .unwrap());
    clear_background(BLACK);
    draw(&first, &pose);
    assert!(center(&read(&target)).abs_diff_eq(queued, 0.01));
    player.set_looping(true);
    player.set_paused(false);
    let looped = player.seek(4.).unwrap();
    telemetry::reset_renderer_diagnostics();
    assert!(!looped.apply(&mut pose, Some(&mut first)).unwrap());
    uploads(0);
    // Transform-only combined updates must leave populated morph geometry,
    // weights and bounds untouched even when an instance is supplied.
    let mut transform_only = clip.sample(1.).unwrap();
    transform_only.weights.clear();
    let mut transform_pose = pose.clone();
    let weights = first.weights(1).unwrap().to_vec();
    let bounds = (first.model().bounds_min, first.model().bounds_max);
    assert!(!transform_only
        .apply(&mut transform_pose, Some(&mut first))
        .unwrap());
    assert_eq!(first.weights(1).unwrap(), weights);
    assert_eq!((first.model().bounds_min, first.model().bounds_max), bounds);
    uploads(0);
    let world = transform_pose
        .world(transform_pose.handle(1).unwrap())
        .unwrap();
    transform_only.transforms[0].1.rotation = Quat::from_array([0.; 4]);
    assert!(transform_only
        .apply(&mut transform_pose, Some(&mut first))
        .is_err());
    assert_eq!(
        transform_pose
            .world(transform_pose.handle(1).unwrap())
            .unwrap(),
        world
    );
    assert_eq!(first.weights(1).unwrap(), weights);
    assert_eq!((first.model().bounds_min, first.model().bounds_max), bounds);
    uploads(0);
    set_default_camera();
    telemetry::set_renderer_diagnostics_enabled(false);
    macroquad::logging::info!("GLTF_ANIMATION_FIXTURE_PASS");
    next_frame().await;
}
