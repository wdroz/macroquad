#![cfg(feature = "gltf")]

#[path = "support/gltf_pose_fixture.rs"]
mod fixture;

// One windowed test per binary: miniquad keeps a process-wide native display.
#[macroquad::test]
async fn moved_gltf_part_replaces_rest_pivot_without_reuploading_geometry() {
    fixture::run().await;
}
