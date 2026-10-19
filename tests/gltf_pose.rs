#![cfg(feature = "gltf")]

#[path = "support/gltf_pose_fixture.rs"]
mod fixture;

// One windowed test per binary: miniquad keeps a process-wide native display.
#[macroquad::test]
async fn moved_gltf_parts_and_hierarchy_instances_reuse_geometry() {
    fixture::run().await;
}
