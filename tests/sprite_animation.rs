use macroquad::{
    experimental::animation::{
        AnimatedSprite, Animation, AnimationError, AnimationFrame, AnimationPlayback, SpriteRegion,
        SpriteRegionDrawParams, SpriteRegionError,
    },
    prelude::*,
};

fn clip(frames: u32, fps: u32) -> Animation {
    Animation {
        name: "test".into(),
        row: 0,
        frames,
        fps,
    }
}

fn player(frames: u32, fps: u32) -> AnimatedSprite {
    AnimatedSprite::try_new(8, 12, &[clip(frames, fps)], true).unwrap()
}

fn snapshot(sprite: &AnimatedSprite) -> (usize, u32, f64, bool, bool, AnimationPlayback) {
    (
        sprite.current_animation(),
        sprite.current_frame(),
        sprite.elapsed(),
        sprite.playing,
        sprite.is_finished(),
        sprite.playback(),
    )
}

#[test]
fn checked_construction_rejects_invalid_metadata_in_every_clip() {
    assert!(matches!(
        AnimatedSprite::try_new(0, 8, &[clip(1, 1)], true),
        Err(AnimationError::ZeroTileSize)
    ));
    assert!(matches!(
        AnimatedSprite::try_new(8, 0, &[clip(1, 1)], true),
        Err(AnimationError::ZeroTileSize)
    ));
    assert!(matches!(
        AnimatedSprite::try_new(8, 8, &[], true),
        Err(AnimationError::EmptyAnimations)
    ));
    assert!(matches!(
        AnimatedSprite::try_new(8, 8, &[clip(1, 1), clip(0, 1)], true),
        Err(AnimationError::EmptyClip { animation: 1 })
    ));
    assert!(matches!(
        AnimatedSprite::try_new(8, 8, &[clip(1, 1), clip(1, 0)], true),
        Err(AnimationError::ZeroFps { animation: 1 })
    ));
}

#[test]
#[should_panic(expected = "invalid sprite animation metadata")]
fn legacy_constructor_reports_bad_metadata_immediately() {
    AnimatedSprite::new(8, 8, &[clip(0, 1)], true);
}

#[test]
fn exact_frame_and_loop_boundaries_include_the_last_frame_duration() {
    let mut sprite = player(4, 8);
    assert_eq!(sprite.playback(), AnimationPlayback::Loop);
    assert_eq!(sprite.duration(), 0.5);
    for frame in 1..4 {
        sprite.update_with_delta(0.125).unwrap();
        assert_eq!(sprite.current_frame(), frame);
        assert_eq!(sprite.elapsed(), f64::from(frame) / 8.);
    }
    assert!(sprite.is_last_frame());
    assert!(!sprite.is_finished());
    sprite.update_with_delta(0.125).unwrap();
    assert_eq!(sprite.current_frame(), 0);
    assert_eq!(sprite.elapsed(), 0.);
}

#[test]
fn nonbinary_fps_boundaries_and_their_predecessors_do_not_need_an_epsilon() {
    for fps in [3, 7, 10, 25, 49, 60, 120, 1001, u32::MAX] {
        let mut sprite = player(65, fps);
        for frame in 1..=65 {
            let boundary = f64::from(frame) / f64::from(fps);
            let before = f64::from_bits(boundary.to_bits() - 1);
            sprite.seek(before).unwrap();
            assert_eq!(sprite.current_frame(), frame - 1, "before {frame}/{fps}");
            let before_state = snapshot(&sprite);
            for _ in 0..4 {
                sprite.update_with_delta(0.).unwrap();
                assert_eq!(
                    snapshot(&sprite),
                    before_state,
                    "zero delta before {frame}/{fps}"
                );
            }
            sprite.seek(boundary).unwrap();
            assert_eq!(sprite.current_frame(), frame % 65, "at {frame}/{fps}");
            assert_eq!(sprite.elapsed(), if frame == 65 { 0. } else { boundary });
            sprite.reset();
            sprite.update_with_delta(boundary).unwrap();
            assert_eq!(sprite.current_frame(), frame % 65, "advance {frame}/{fps}");
        }
    }
}

#[test]
fn long_ticks_consume_multiple_cycles_and_retain_fractional_time() {
    let mut sprite = player(4, 8);
    sprite.update_with_delta(1000.3125).unwrap();
    assert_eq!(sprite.current_frame(), 2);
    assert_eq!(sprite.elapsed(), 0.3125);
    sprite.update_with_delta(0.0625).unwrap();
    assert_eq!(sprite.current_frame(), 3);
    assert_eq!(sprite.elapsed(), 0.375);
}

#[test]
fn representable_subdivisions_match_one_tick() {
    let mut whole = player(7, 8);
    let mut subdivided = whole.clone();
    whole.update_with_delta(123.8125).unwrap();
    for _ in 0..1981 {
        subdivided.update_with_delta(0.0625).unwrap();
    }
    assert_eq!(snapshot(&whole), snapshot(&subdivided));
}

#[test]
fn pause_resume_retains_the_partial_frame() {
    let mut sprite = player(4, 8);
    sprite.update_with_delta(0.0625).unwrap();
    sprite.playing = false;
    let paused = snapshot(&sprite);
    sprite.update_with_delta(f64::MAX).unwrap();
    assert_eq!(snapshot(&sprite), paused);
    sprite.playing = true;
    sprite.update_with_delta(0.0625).unwrap();
    assert_eq!(sprite.current_frame(), 1);
    assert_eq!(sprite.elapsed(), 0.125);
}

#[test]
fn one_shot_finishes_after_terminal_dwell_and_holds_until_repositioned() {
    let mut sprite = player(4, 8);
    sprite.set_playback(AnimationPlayback::Once);
    sprite.update_with_delta(0.375).unwrap();
    assert!(sprite.is_last_frame());
    assert!(!sprite.is_finished());
    sprite.update_with_delta(0.0625).unwrap();
    assert!(!sprite.is_finished());
    sprite.update_with_delta(0.0625).unwrap();
    assert!(sprite.is_finished());
    assert!(sprite.playing);
    assert_eq!(sprite.elapsed(), 0.5);
    let complete = snapshot(&sprite);
    sprite.update_with_delta(f64::MAX).unwrap();
    sprite.set_playback(AnimationPlayback::Once);
    assert_eq!(snapshot(&sprite), complete);
    sprite.reset();
    assert!(!sprite.is_finished());
    assert!(sprite.playing);
    assert_eq!(sprite.playback(), AnimationPlayback::Once);
    sprite.update_with_delta(f64::MAX).unwrap();
    assert!(sprite.is_finished());
}

#[test]
fn single_frame_one_shot_has_a_nonzero_duration() {
    let mut sprite = player(1, 4);
    sprite.set_playback(AnimationPlayback::Once);
    assert!(sprite.is_last_frame());
    assert!(!sprite.is_finished());
    sprite.update_with_delta(0.125).unwrap();
    assert!(!sprite.is_finished());
    sprite.update_with_delta(0.125).unwrap();
    assert!(sprite.is_finished());
    sprite.set_playback(AnimationPlayback::Loop);
    assert_eq!(sprite.elapsed(), 0.);
    assert!(!sprite.is_finished());
    sprite.update_with_delta(10.125).unwrap();
    assert_eq!(sprite.elapsed(), 0.125);
}

#[test]
fn seek_wraps_or_clamps_and_can_rewind_a_paused_completed_player() {
    let mut sprite = player(4, 8);
    sprite.playing = false;
    sprite.seek(10.3125).unwrap();
    assert_eq!(sprite.elapsed(), 0.3125);
    sprite.set_playback(AnimationPlayback::Once);
    assert_eq!(sprite.elapsed(), 0.3125);
    sprite.seek(0.5).unwrap();
    assert!(sprite.is_finished());
    assert!(!sprite.playing);
    sprite.seek(0.1875).unwrap();
    assert!(!sprite.is_finished());
    assert_eq!(sprite.current_frame(), 1);
    assert_eq!(sprite.elapsed(), 0.1875);
    sprite.reset();
    assert_eq!(sprite.elapsed(), 0.);
    assert!(!sprite.playing);
    assert_eq!(sprite.playback(), AnimationPlayback::Once);
}

#[test]
fn invalid_time_is_transactional_while_playing_paused_and_finished() {
    for state in 0..3 {
        let mut sprite = player(4, 8);
        sprite.seek(0.1875).unwrap();
        if state == 1 {
            sprite.playing = false;
        } else if state == 2 {
            sprite.set_playback(AnimationPlayback::Once);
            sprite.seek(sprite.duration()).unwrap();
        }
        let before = snapshot(&sprite);
        for invalid in [-0.1, -f64::MAX, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert_eq!(
                sprite.update_with_delta(invalid),
                Err(AnimationError::InvalidTime)
            );
            assert_eq!(snapshot(&sprite), before);
            assert_eq!(sprite.seek(invalid), Err(AnimationError::InvalidTime));
            assert_eq!(snapshot(&sprite), before);
        }
        sprite.update_with_delta(-0.0).unwrap();
        assert_eq!(snapshot(&sprite), before);
    }
}

#[test]
fn extreme_finite_deltas_do_not_overflow_and_preserve_existing_fraction() {
    let mut sprite = player(7, u32::MAX);
    let mut sought = sprite.clone();
    sprite.update_with_delta(f64::MAX).unwrap();
    sought.seek(f64::MAX).unwrap();
    let expected = (((f64::MAX % 7.) as u64 * u64::from(u32::MAX)) % 7) as u32;
    assert_eq!(sprite.current_frame(), expected);
    assert_eq!(snapshot(&sprite), snapshot(&sought));
    assert!(sprite.elapsed().is_finite());
    let mut slow = player(u32::MAX, 1);
    slow.update_with_delta(0.5).unwrap();
    slow.update_with_delta(f64::MAX).unwrap();
    assert_eq!(
        slow.current_frame(),
        (f64::MAX % f64::from(u32::MAX)) as u32
    );
    slow.update_with_delta(0.5).unwrap();
    assert_eq!(
        slow.current_frame(),
        ((f64::MAX % f64::from(u32::MAX)) as u32 + 1) % u32::MAX
    );
    let mut largest = player(u32::MAX, u32::MAX);
    largest.update_with_delta(f64::MAX).unwrap();
    assert_eq!(largest.current_frame(), 0);
    assert_eq!(largest.elapsed(), 0.);
}

#[test]
fn clones_and_separate_instances_own_independent_clocks() {
    let mut first = player(4, 8);
    first.update_with_delta(0.1875).unwrap();
    let mut cloned = first.clone();
    let separate = player(4, 8);
    cloned.update_with_delta(0.125).unwrap();
    cloned.set_playback(AnimationPlayback::Once);
    assert_eq!(first.elapsed(), 0.1875);
    assert_eq!(cloned.elapsed(), 0.3125);
    assert_eq!(separate.elapsed(), 0.);
    assert_eq!(first.playback(), AnimationPlayback::Loop);
    assert_eq!(cloned.playback(), AnimationPlayback::Once);
}

#[test]
fn legacy_setters_keep_fractional_seconds_and_clear_completion() {
    let mut second = clip(2, 16);
    second.row = 3;
    let mut sprite = AnimatedSprite::new(8, 12, &[clip(4, 4), second], true);
    sprite.seek(0.875).unwrap(); // frame 3 and 0.125 seconds of its duration
    sprite.set_animation(1);
    assert_eq!(sprite.current_frame(), 1);
    assert_eq!(sprite.elapsed(), 0.1875);
    sprite.update_with_delta(0.).unwrap(); // consumes the two retained frames
    assert_eq!(sprite.current_frame(), 1);
    assert_eq!(sprite.elapsed(), 0.0625);
    sprite.set_frame(0);
    assert_eq!(sprite.frame().source_rect, Rect::new(0., 36., 8., 12.));
    sprite.set_playback(AnimationPlayback::Once);
    sprite.seek(5.).unwrap();
    sprite.set_frame(0);
    assert!(!sprite.is_finished());
    assert_eq!(sprite.elapsed(), 0.);
    sprite.seek(5.).unwrap();
    sprite.set_animation(0);
    assert!(!sprite.is_finished());
    assert_eq!(sprite.playback(), AnimationPlayback::Once);
}

#[test]
fn invalid_animation_selection_does_not_poison_a_recovered_player() {
    let mut sprite = player(4, 8);
    sprite.seek(0.1875).unwrap();
    let before = snapshot(&sprite);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sprite.set_animation(usize::MAX);
    }));
    assert!(
        result.is_err(),
        "the legacy setter still rejects invalid indices"
    );
    assert_eq!(snapshot(&sprite), before);
    assert_eq!(sprite.frame().source_rect, Rect::new(8., 0., 8., 12.));
    sprite.update_with_delta(0.0625).unwrap();
    assert_eq!(sprite.current_frame(), 2);
    assert_eq!(sprite.elapsed(), 0.25);
}

#[test]
fn playback_mode_changes_preserve_progress_and_wrap_completed_once() {
    let mut sprite = player(4, 8);
    sprite.seek(0.1875).unwrap();
    sprite.set_playback(AnimationPlayback::Once);
    assert_eq!(sprite.elapsed(), 0.1875);
    sprite.set_playback(AnimationPlayback::Loop);
    assert_eq!(sprite.elapsed(), 0.1875);
    sprite.set_playback(AnimationPlayback::Once);
    sprite.seek(1.).unwrap();
    sprite.playing = false;
    sprite.set_playback(AnimationPlayback::Loop);
    assert_eq!(sprite.current_frame(), 0);
    assert_eq!(sprite.elapsed(), 0.);
    assert!(!sprite.is_finished());
    assert!(!sprite.playing);
}

#[test]
fn frame_literals_and_regions_preserve_source_pixels_and_anchored_flips() {
    let animation = Animation {
        name: "row".into(),
        row: 1,
        frames: 4,
        fps: 4,
    };
    let mut sprite = AnimatedSprite::try_new(8, 12, &[animation], true).unwrap();
    let anchor = vec2(30., 50.);
    for frame in 0..4 {
        sprite.seek(f64::from(frame) / 4.).unwrap();
        let AnimationFrame {
            source_rect,
            dest_size,
        } = sprite.frame();
        assert_eq!(source_rect, Rect::new(frame as f32 * 8., 12., 8., 12.));
        assert_eq!(dest_size, vec2(8., 12.));
        let region = SpriteRegion::new(vec2(32., 24.), source_rect)
            .unwrap()
            .with_pivot(vec2(4., 12.))
            .unwrap();
        for flip_x in [false, true] {
            for flip_y in [false, true] {
                let (position, params) = region
                    .draw_params(
                        anchor,
                        SpriteRegionDrawParams {
                            flip_x,
                            flip_y,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                assert_eq!(params.source, Some(source_rect));
                assert_eq!(params.pivot, Some(anchor));
                assert_eq!(position, vec2(26., if flip_y { 50. } else { 38. }));
            }
        }
    }
    assert_eq!(
        SpriteRegion::new(vec2(16., 12.), sprite.frame().source_rect),
        Err(SpriteRegionError::SourceOutOfBounds)
    );
}
