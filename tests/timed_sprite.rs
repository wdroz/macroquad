use macroquad::experimental::animation::*;
use macroquad::prelude::*;

fn region(index: u32) -> SpriteRegion {
    SpriteRegion::from_grid(vec2(24., 8.), vec2(8., 8.), index, 0).unwrap()
}

fn player() -> TimedSprite {
    TimedSprite::new(
        SpriteClip::new(&[(region(0), 0.125), (region(1), 0.375), (region(2), 0.25)]).unwrap(),
        true,
    )
}

#[test]
fn invalid_clips_and_unrepresentable_totals() {
    assert_eq!(SpriteClip::new(&[]).unwrap_err(), SpriteClipError::Empty);
    for time in [0., -0., -1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            SpriteClip::new(&[(region(0), 0.5), (region(1), time)]).unwrap_err(),
            SpriteClipError::InvalidDuration { frame: 1 }
        );
    }
    for durations in [[f64::MAX, f64::MAX], [1., f64::MIN_POSITIVE]] {
        assert_eq!(
            SpriteClip::new(&[(region(0), durations[0]), (region(1), durations[1])]).unwrap_err(),
            SpriteClipError::UnusableTotal { frame: 1 }
        );
    }
}

#[test]
fn exact_boundaries_and_their_representable_neighbors() {
    let mut p = player();
    for (boundary, before, at) in [(0.125_f64, 0, 1), (0.5, 1, 2), (0.75, 2, 0)] {
        p.seek(f64::from_bits(boundary.to_bits() - 1)).unwrap();
        assert_eq!(p.current_frame(), before);
        p.seek(boundary).unwrap();
        assert_eq!(p.current_frame(), at);
        p.seek(f64::from_bits(boundary.to_bits() + 1)).unwrap();
        assert_eq!(p.current_frame(), at);
    }
}

#[test]
fn decimal_boundaries_use_the_stored_accumulated_sum() {
    let clip = SpriteClip::new(&[(region(0), 0.1), (region(1), 0.2), (region(2), 0.4)]).unwrap();
    assert_eq!(clip.duration(), (0.1 + 0.2) + 0.4);
    let mut p = TimedSprite::new(clip, true);
    let boundary = 0.1_f64 + 0.2;
    p.seek(f64::from_bits(boundary.to_bits() - 1)).unwrap();
    assert_eq!(p.current_frame(), 1);
    p.seek(boundary).unwrap();
    assert_eq!(p.current_frame(), 2);
}

#[test]
fn single_frame_still_has_a_complete_interval() {
    let mut p = TimedSprite::new(SpriteClip::new(&[(region(1), 0.25)]).unwrap(), true);
    p.update_with_delta(100.125).unwrap();
    assert_eq!(p.current_frame(), 0);
    assert_eq!(p.elapsed(), 0.125);
    p.set_playback(AnimationPlayback::Once);
    p.update_with_delta(0.).unwrap();
    assert!(!p.is_finished());
    p.update_with_delta(0.125).unwrap();
    assert!(p.is_finished());
    p.reset();
    assert!(!p.is_finished());
    assert_eq!(p.playback(), AnimationPlayback::Once);
    assert!(p.playing);
}

#[test]
fn fractional_carry_and_many_loops() {
    let mut p = player();
    p.update_with_delta(0.0625).unwrap();
    p.update_with_delta(0.0625).unwrap();
    assert_eq!(p.current_frame(), 1);
    p.update_with_delta(0.75 * 1024. + 0.3125).unwrap();
    assert_eq!(p.current_frame(), 1);
    assert_eq!(p.elapsed(), 0.4375);
    p.update_with_delta(0.0625).unwrap();
    assert_eq!(p.current_frame(), 2);
    p.update_with_delta(0.25).unwrap();
    assert_eq!(p.elapsed(), 0.);
}

#[test]
fn once_holds_last_frame_for_its_complete_duration() {
    let mut p = player();
    p.set_playback(AnimationPlayback::Once);
    p.seek(0.5).unwrap();
    assert_eq!(p.current_frame(), 2);
    assert!(!p.is_finished());
    p.update_with_delta(0.125).unwrap();
    assert!(!p.is_finished());
    p.update_with_delta(0.125).unwrap();
    assert!(p.is_finished());
    assert_eq!(p.elapsed(), 0.75);
    assert_eq!(p.frame(), &region(2));
    p.update_with_delta(f64::MAX).unwrap();
    assert_eq!(p.elapsed(), 0.75);
    p.seek(0.125).unwrap();
    assert!(!p.is_finished());
    p.seek(f64::MAX).unwrap();
    assert!(p.is_finished());
}

#[test]
fn pause_reset_mode_changes_and_independent_clones() {
    let mut a = player();
    a.update_with_delta(0.0625).unwrap();
    let mut b = a.clone();
    a.playing = false;
    a.update_with_delta(10.).unwrap();
    b.update_with_delta(0.4375).unwrap();
    assert_eq!(a.elapsed(), 0.0625);
    assert_eq!(b.current_frame(), 2);
    a.seek(0.5).unwrap();
    assert!(!a.playing);
    a.set_playback(AnimationPlayback::Once);
    a.seek(a.duration()).unwrap();
    a.set_playback(AnimationPlayback::Loop);
    assert_eq!(a.elapsed(), 0.);
    assert!(!a.playing);
    a.seek(0.625).unwrap();
    a.reset();
    assert_eq!(a.current_frame(), 0);
    assert_eq!(a.playback(), AnimationPlayback::Loop);
    assert_eq!(b.elapsed(), 0.5);
}

#[test]
fn invalid_times_are_transactional_even_paused_or_finished() {
    for state in 0..3 {
        let mut p = player();
        p.seek(0.4375).unwrap();
        if state == 1 {
            p.playing = false;
        }
        if state == 2 {
            p.set_playback(AnimationPlayback::Once);
            p.seek(1.).unwrap();
        }
        let before = format!("{p:?}");
        for time in [-1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(p.seek(time), Err(AnimationError::InvalidTime));
            assert_eq!(p.update_with_delta(time), Err(AnimationError::InvalidTime));
            assert_eq!(format!("{p:?}"), before);
        }
    }
}

#[test]
fn huge_and_subnormal_times_do_not_overflow() {
    let tiny = f64::from_bits(1);
    let mut p = TimedSprite::new(
        SpriteClip::new(&[(region(0), tiny), (region(1), tiny)]).unwrap(),
        true,
    );
    p.update_with_delta(tiny).unwrap();
    assert_eq!(p.current_frame(), 1);
    p.update_with_delta(f64::MAX).unwrap();
    assert_eq!(p.current_frame(), 1);
    let mut p = TimedSprite::new(
        SpriteClip::new(&[(region(0), f64::MAX / 2.), (region(1), f64::MAX / 2.)]).unwrap(),
        true,
    );
    p.seek(f64::MAX / 2.).unwrap();
    p.update_with_delta(f64::MAX).unwrap();
    assert_eq!(p.current_frame(), 1);
    assert!(p.elapsed().is_finite() && p.elapsed() < p.duration());
}

#[test]
fn regions_preserve_asymmetric_trim_pivot_and_uniform_api_literals() {
    let frame = SpriteRegion::trimmed(
        vec2(40., 20.),
        Rect::new(25., 2., 10., 14.),
        vec2(20., 20.),
        vec2(5., 2.),
    )
    .unwrap()
    .with_pivot(vec2(10., 10.))
    .unwrap();
    let mut p = TimedSprite::new(
        SpriteClip::new(&[(region(0), 0.125), (frame, 0.375)]).unwrap(),
        true,
    );
    p.seek(0.125).unwrap();
    assert_eq!(p.frame(), &frame);
    assert_eq!(p.clip().frames()[1], (frame, 0.375));
    let legacy = Animation {
        name: "legacy".into(),
        row: 0,
        frames: 3,
        fps: 8,
    };
    let mut old = AnimatedSprite::new(8, 8, &[legacy], true);
    old.update_with_delta(0.25).unwrap();
    let AnimationFrame {
        source_rect,
        dest_size,
    } = old.frame();
    assert_eq!(source_rect.x, 16.);
    assert_eq!(dest_size, vec2(8., 8.));
}
