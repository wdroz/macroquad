# macroquad

[![Github Actions](https://github.com/not-fl3/macroquad/workflows/CI/badge.svg)](https://github.com/not-fl3/macroquad/actions?query=workflow%3A)
[![Docs](https://docs.rs/macroquad/badge.svg?version=0.4.5)](https://docs.rs/macroquad/0.4.5/macroquad/index.html)
[![Crates.io version](https://img.shields.io/crates/v/macroquad.svg)](https://crates.io/crates/macroquad)
[![Discord chat](https://img.shields.io/discord/710177966440579103.svg?label=discord%20chat)](https://discord.gg/WfEp6ut)

`macroquad` is a simple and easy to use game library for Rust programming language, heavily inspired by [raylib](https://github.com/raysan5/raylib).

## Features

* Same code for all supported platforms, no platform dependent defines required.
* Efficient 2D rendering with automatic geometry batching.
* Minimal amount of dependencies: build after `cargo clean` takes only 16s on x230(~6 years old laptop).
* Immediate mode UI library included.
* Single command deploy for both WASM and Android.

## Default alpha compositing (fork fix)

The default drawing and built-in UI pipelines preserve source-over alpha independently of
RGB blending: `A_out = A_source + A_destination * (1 - A_source)`. This keeps
antialiased text and translucent UI opaque when drawn over an opaque canvas,
so WebGL page compositing matches native rendering. Transparent render targets
still accumulate coverage correctly. Custom materials continue to use exactly
the `PipelineParams` supplied to `load_material`; their blend settings are not
overridden. The GPU regression is `cargo test --test default_alpha_blending`.

## Queued text atlas lifetime (fork fix)

Growing a font atlas or dropping its last font handle preserves the texture
and UVs already captured by pending text draws. Replaced atlas textures are
retired after frame submission, using the existing texture collection boundary.
No application flush or extra font clone is required for this lifetime case.
This does not change atlas packing, glyph rasterization, or public texture
ownership rules. Run `cargo test --test font_atlas_lifetime` or the
`font_atlas_lifetime` example for a GPU pixel regression; the example also runs
in WebAssembly with actual high-DPI text.

## Textured glTF scenes (fork feature)

Enable `features = ["gltf"]` to use the CPU-only `GltfScene` parser and the
`GltfModel` rendering adapter. `GltfModel::from_bytes(&bytes)` loads embedded
PNG/JPEG/WebP base-color and emissive textures, applies node world transforms and normal
matrices, and splits large index buffers into triangle-safe 16-bit chunks.
`GltfModel::with_up_axis` rotates those baked vertices the same way
`GltfScene::with_up_axis` rotates markers. `Vec3::Y` leaves both unchanged.
Call `from_bytes` after macroquad starts, because it creates GPU textures. `model.draw()`
submits the textured meshes with the current camera and material.

For memory-constrained scenes, `GltfModel::from_bytes_with_options` accepts
`GltfLoadOptions { max_texture_size: Some(512), ..Default::default() }`.
This bounds both edges of each uploaded base-color, emissive, normal, and
metallic-roughness image, preserves
aspect ratio, and never enlarges an image. The default is `None`: existing
`from_bytes` and `from_bytes_with_limits` calls retain full resolution. The
cap reduces GPU texture and mipmap memory; the original image is still decoded
temporarily in CPU memory. Source GLBs are never modified.

```sh
cargo run --features gltf --example gltf_textured
cargo run --features gltf --example gltf_textured -- path/to/model.glb
cargo run --features gltf --example gltf_retained
cargo run --features gltf --example gltf_pollable
cargo run --features gltf --example gltf_materials
cargo run --features gltf --example gltf_shading
cargo run --features gltf --example gltf_pose_check
```

`model.parts` exposes each mesh, source node/name, bounds, and surface metadata
(alpha mode/cutoff, sidedness, emissive, metallic, roughness and transmission values).
A custom shader can use that metadata for its own lighting and transparency order.
The base color factor is already in the vertices at 8-bit precision; a shader can use
`surface.base_color` for exact factors, including alpha-mask cutoff comparisons.
`GltfModel::draw` and `RetainedGltfModel::draw` do not
implement PBR, skins, animations, morph targets, or alpha sorting. Only embedded
GLB images are supported. The base-color texture selects its authored UV set;
`KHR_texture_transform` offset, rotation, scale and UV-set override are applied.
`EXT_texture_webp` is resolved in memory without modifying the source file;
required unlit/emissive-strength metadata is preserved for custom shaders.
Other unknown required extensions still fail validation. Image samplers preserve
repeat/clamp modes and minification/magnification filters, generating mipmaps
when requested (trilinear filtering when unspecified), preserving image pixels
and dimensions. WebGL 1 / GLES 2 non-power-of-two textures fall back to linear
clamp sampling; use WebGL 2 for full NPOT sampler behavior.
`Texture2D::from_rgba8_mipmapped` also exposes managed mipmapped texture creation
with preallocated mip levels for Metal.

The separate CPU-only `macroquad::gltf::GltfMorphData` sidecar imports
POSITION/NORMAL morph deltas and default weights and evaluates explicit weights
with deformed bounds. It does not change these draw methods; see the
[morph evaluation guide](docs/3d-guide.md#cpu-morph-data-and-evaluation).

`GltfModel::from_bytes_with_pose` (or `GltfModelLoader::take_pose`) keeps the
authored node matrices. `GltfNodePose::set_world` replaces one node's world
matrix and draws around that node's origin. `GltfModelPose::hierarchy` edits
local translation, rotation, and scale and updates descendants. Spawn markers
and the stored collider list stay at rest. `GltfHierarchyPose::collider_shape`
returns one collider in the current worlds. See
[docs/3d-guide.md](docs/3d-guide.md#replacing-an-imported-node-pose).

For static geometry, `RetainedGltfModel::from_model(model, false)` consumes the
finished CPU model, uploads each chunk once, and releases its CPU geometry.
Pass `true` to keep a shared CPU model for inspection. Clones share GPU buffers
and textures; `draw(transform)` submits an instance, and `draw_culled(transform,
&frustum)` skips chunks outside a camera frustum. These are ordinary draw calls
with shared geometry, not hardware instancing. Custom lighting, transparency
sorting, normal transforms and reflected face culling remain caller controlled;
`GltfModelTransform` supplies the inverse-transpose matrix and mirrored flag.
For a hand-built `Mesh`, use `RetainedMesh` and `draw_retained_mesh` instead.

`GltfMaterialRenderer` collects those parts without a custom shader.
`submit` and `submit_retained` draw every chunk of a CPU or retained model.
`submit_part` and `submit_retained_part` borrow one existing chunk and apply
an extra affine transform, so hiding or moving one object does not clone the
model. Opaque and masked chunks draw before one back-to-front blend pass.
`set_lighting(Some(GltfLighting { .. }))` adds hemisphere and directional
diffuse light from imported normals. Unlit materials ignore it, and the
existing `draw` methods stay available. Normal and metallic-roughness images,
UV transforms, and tangents are retained by
`GltfModel::from_bytes_with_material_maps` (and on the CPU by
`GltfMaterialMaps`). The opt-in `submit_shaded` methods sample these maps with
hemisphere diffuse and directional GGX lighting; `GltfShadingModel` retains
the extra attributes for GPU drawing. Occlusion, environment lighting,
refraction, and shadows are not evaluated. `examples/gltf_materials.rs`
toggles diffuse lighting, `examples/gltf_shading.rs` shows map sampling, and
`examples/gltf_material_check.rs` checks both paths and selected parts.

`GltfModelLoader` and `RetainedGltfModelLoader` perform the same construction
one stage at a time. Parsing, one image decode, and one mesh conversion can
still occupy a frame. `GltfLoadSettings` adds per-image source-pixel and
estimated RGBA8 byte limits checked before decode; `max_texture_size` remains
the later upload cap. After a scene change, `retire_unused_render_resources`
flushes queued draws and collects unused textures and retained buffers.
`GltfResourceReport` and `telemetry::renderer_diagnostics` report estimated
bytes and upload counts, not process or driver memory.

`CursorCapture` reports whether `set_cursor_grab` is idle, requested, active,
denied, or lost. The browser bundle asks for Pointer Lock once per gesture and
does not repeat the request every frame. `mouse_position_logical` /
`mouse_position_physical`, the matching touch accessors, and
`logical_to_physical` / `physical_to_logical` separate logical window units
from framebuffer pixels. `Camera3D::screen_to_ray` takes the logical window
point. `examples/cursor_capture.rs` and `examples/input_coordinates.rs` show
the two APIs. On the web, optional density, framebuffer, and frame-rate caps
live in [docs/browser-host.md](docs/browser-host.md); `examples/canvas_host.rs`
is the fixture.

`Aabb`, `Sphere`, `Capsule`, `Ray`, `Plane`, `Triangle`, `Obb`, and `Frustum` live in
`macroquad::math` and the prelude. `Contact` describes an overlapping pair,
`SweepHit` describes a moving box against an AABB or a sphere against an AABB/OBB,
`slide_sphere` keeps the leftover motion along a face, and
`slide_sphere_solids` does the same against spheres and capsules. There is no rigid-body
world and no character controller. [docs/3d-guide.md](docs/3d-guide.md) covers
the queries and the application code a game writes for gravity and a capsule
on a floor. [docs/blender-to-macroquad.md](docs/blender-to-macroquad.md)
is the export and loading tutorial, including `with_up_axis` and the
collider / trigger / spawn names. Bot-added examples are listed in
[examples/README.md](examples/README.md).

`GltfScene::marker_rotation(spawn.node)` supplies a normalized world rotation
for an imported spawn, including parent rotation and `with_up_axis` conversion.
The application chooses its local forward axis; the existing `GltfSpawn`
fields stay unchanged. Missing nodes and undecomposable transforms return
`None`. Shear uses an approximate matrix decomposition; see the
[marker orientation guide](docs/3d-guide.md#loading-and-materials).

The default mesh chunk limits fit macroquad's default buffers. For larger
chunks, pass `GltfMeshLimits` to `from_bytes_with_limits` and increase both
`draw_call_vertex_capacity` and `draw_call_index_capacity` in the app's config
when drawing immediately. Retained geometry does not use the streaming buffer
capacities, but still validates triangle indices and the 16-bit vertex range.

`Camera3D::screen_to_ray` supplies a world ray for picking;
`Camera3D::frustum` supplies conservative culling planes. Both use the camera's
view-projection and respect its chosen up axis. See the guide for viewport and
framebuffer coordinates and for the difference between containment and culling.

## Video textures (fork feature)

Enable `features = ["video"]` for `video::VideoPlayer`, independently of the
`audio` feature. Call `update` each frame before drawing its ordinary `Texture2D`
on a 2D surface or 3D screen. Native playback uses FFmpeg/FFprobe with bounded
decode queues; native audio currently requires Windows. Browsers use the
`js/mq_video.js` companion and browser media codecs. See [docs/video.md](docs/video.md)
for setup, playback controls, cleanup, and platform limitations.

## Supported Platforms

* PC: Windows/Linux/macOS;
* HTML5;
* Android;
* IOS.

## Build Instructions

### Setting Up a Macroquad Project

Macroquad is a normal rust dependency, therefore an empty macroquad project may be created with:

```sh
# Create empty cargo project
cargo init --bin
```

Add macroquad as a dependency to Cargo.toml:
```toml

[dependencies]
macroquad = "0.4"
```

Put some macroquad code in `src/main.rs`:
```rust
use macroquad::prelude::*;

#[macroquad::main("BasicShapes")]
async fn main() {
    loop {
        clear_background(RED);

        draw_line(40.0, 40.0, 100.0, 200.0, 15.0, BLUE);
        draw_rectangle(screen_width() / 2.0 - 60.0, 100.0, 120.0, 60.0, GREEN);
        draw_circle(screen_width() - 30.0, screen_height() - 30.0, 15.0, YELLOW);

        draw_text("IT WORKS!", 20.0, 20.0, 30.0, DARKGRAY);

        next_frame().await
    }
}
```

And to run it natively:
```sh
cargo run
```

For more examples take a look at [Macroquad examples folder](https://github.com/not-fl3/macroquad/tree/master/examples)

### Linux

```sh
# ubuntu system dependencies
apt install pkg-config libx11-dev libxi-dev libgl1-mesa-dev libasound2-dev

# fedora system dependencies
dnf install libX11-devel libXi-devel mesa-libGL-devel alsa-lib-devel

# arch linux system dependencies
pacman -S pkg-config libx11 libxi mesa-libgl alsa-lib
```

### Windows

On windows both MSVC and GNU target are supported, no additional dependencies required.

Also cross-compilation to windows from linux is supported:

```sh
rustup target add x86_64-pc-windows-gnu

cargo run --target x86_64-pc-windows-gnu
```

### WASM

```sh
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown
```

This will produce .wasm file in `target/wasm32-unknown-unknown/debug/CRATENAME.wasm` or in `target/wasm32-unknown-unknown/release/CRATENAME.wasm` if built with `--release`.

And then use the following .html to load it:

<details><summary>index.html</summary>

```html
<html lang="en">

<head>
    <meta charset="utf-8">
    <title>TITLE</title>
    <style>
        html,
        body,
        canvas {
            margin: 0px;
            padding: 0px;
            width: 100%;
            height: 100%;
            overflow: hidden;
            position: absolute;
            background: black;
            z-index: 0;
        }
    </style>
</head>

<body>
    <canvas id="glcanvas" tabindex='1'></canvas>
    <!-- Minified and statically hosted version of https://github.com/not-fl3/macroquad/blob/master/js/mq_js_bundle.js -->
    <script src="https://not-fl3.github.io/miniquad-samples/mq_js_bundle.js"></script>
    <script>load("CRATENAME.wasm");</script> <!-- Your compiled wasm file -->
</body>

</html>
```
</details>

One of the ways to server static .wasm and .html:

```sh
cargo install basic-http-server
basic-http-server .
```

### IOS

To run on the simulator:

```sh
mkdir MyGame.app
cargo build --target x86_64-apple-ios --release
cp target/x86_64-apple-ios/release/mygame MyGame.app
# only if the game have any assets
cp -r assets MyGame.app
cat > MyGame.app/Info.plist << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
<key>CFBundleExecutable</key>
<string>mygame</string>
<key>CFBundleIdentifier</key>
<string>com.mygame</string>
<key>CFBundleName</key>
<string>mygame</string>
<key>CFBundleVersion</key>
<string>1</string>
<key>CFBundleShortVersionString</key>
<string>1.0</string>
</dict>
</plist>
EOF

xcrun simctl install booted MyGame.app/
xcrun simctl launch booted com.mygame
```

For details and instructions on provisioning for real iphone, check [https://macroquad.rs/articles/ios/](https://macroquad.rs/articles/ios/)

<details>
<summary>Tips</summary>
Adding the following snippet to your Cargo.toml ensures that all dependencies compile in release even in debug mode. In macroquad, this has the effect of making images load several times faster and your applications much more performant, while keeping compile times miraculously low.

```toml
[profile.dev.package.'*']
opt-level = 3
```
</details>

## async/await

While macroquad attempts to use as few Rust-specific concepts as possible, `.await` in all examples looks a bit scary.
Rust's `async/await` is used to solve just one problem - cross platform main loop organization.

<details>
<summary>Details</summary>


The problem: on WASM and android it's not really easy to organize the main loop like this:
```rust
fn main() {
    // do some initialization

    // start main loop
    loop {
        // handle input

        // update logic

        // draw frame
    }
}
```

It is fixable on Android with threads, but on web there is not way to "pause" and "resume" WASM execution, so no WASM code should block ever.
While that loop is blocking for the entire game execution!
The C++ solution for that problem: https://kripken.github.io/blog/wasm/2019/07/16/asyncify.html

But in Rust we have async/await. Rust's `futures` are basically continuations - `future`'s stack may be stored into a variable to pause/resume execution of future's code at a later point.

async/await support in macroquad comes without any external dependencies - no runtime, no executors and futures-rs is not involved. It's just a way to preserve `main`'s stack on WASM and keep the code cross platform without any WASM-specific main loop.
</Details>

## Community

- [Quads Discord server](https://discord.gg/WfEp6ut) - a place to chat with the library's devs and other community members.
- [Awesome Quads](https://github.com/ozkriff/awesome-quads) - a curated list of links to miniquad/macroquad-related code & resources.

# Platinum sponsors

Macroquad is supported by:

[SourceGear](https://www.sourcegear.com/)
