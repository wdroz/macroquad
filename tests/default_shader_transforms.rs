use macroquad::prelude::*;

#[track_caller]
fn assert_pixel(image: &Image, x: usize, y: usize, expected: [u8; 4]) {
    let offset = (y * image.width as usize + x) * 4;
    assert_eq!(
        &image.bytes[offset..offset + 4],
        &expected,
        "pixel ({x}, {y})"
    );
}

// This uses only the default shader, so the draw itself is portable to Metal.
// Automated readback currently requires OpenGL/WebGL: miniquad 0.4.11 does
// not implement texture_read_pixels on Metal. Source/byte-layout tests cover
// its uniform declaration separately; this test is not Metal runtime evidence.
#[macroquad::test]
async fn default_shader_composes_projection_parent_and_instance_in_order() {
    let target = render_target(64, 64);
    set_camera(&Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 64., 64.))
    });
    let mesh = Mesh {
        vertices: vec![
            Vertex::new(0., 0., 0., 0., 0., WHITE),
            Vertex::new(20., 0., 0., 1., 0., WHITE),
            Vertex::new(20., 8., 0., 1., 1., WHITE),
            Vertex::new(0., 8., 0., 0., 1., WHITE),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    };
    let retained = RetainedMesh::from_mesh(&mesh).unwrap();
    let parent = Mat4::from_translation(vec3(32., 12., 0.)) * Mat4::from_scale(vec3(2., 2., 1.));
    let instance = Mat4::from_translation(vec3(4., 0., 0.))
        * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);
    for use_retained in [false, true] {
        clear_background(BLACK);
        unsafe { get_internal_gl().quad_gl.push_model_matrix(parent) };
        if use_retained {
            draw_retained_mesh(&retained, instance);
        } else {
            unsafe { get_internal_gl().quad_gl.push_model_matrix(instance) };
            draw_mesh(&mesh);
            unsafe { get_internal_gl().quad_gl.pop_model_matrix() };
        }
        unsafe { get_internal_gl().quad_gl.pop_model_matrix() };
        // A following immediate draw also catches leaked transform state.
        draw_rectangle(0., 0., 8., 64., Color::new(0., 0., 1., 1.));
        retire_unused_render_resources();
        let image = target.texture.get_texture_data();
        // Analytically, rotating [0,20] x [0,8], translating by (4,0),
        // scaling by two and translating by (32,12) gives [24,40] x [12,52].
        // Both axes are symmetric about the target center, so pixel origins
        // cannot hide an incorrect transform or introduce backend ambiguity.
        for y in [16, 32, 48] {
            assert_pixel(&image, 4, y, [0, 0, 255, 255]);
            assert_pixel(&image, 16, y, [0, 0, 0, 255]);
            assert_pixel(&image, 32, y, [255, 255, 255, 255]);
            assert_pixel(&image, 48, y, [0, 0, 0, 255]);
        }
        assert_pixel(&image, 32, 8, [0, 0, 0, 255]);
        assert_pixel(&image, 32, 56, [0, 0, 0, 255]);
    }
    set_default_camera();
}
