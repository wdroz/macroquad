//! A tiny original marked level built from the existing checkerboard asset.

use std::borrow::Cow;

/// Includes source nodes outside the selected scene, duplicate marker names,
/// parented facing, all four collider shapes and a tag-only node.
pub fn bytes() -> Vec<u8> {
    let source =
        gltf::binary::Glb::from_slice(include_bytes!("../assets/textured_quad.glb")).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&source.json).unwrap();
    json["scene"] = serde_json::json!(1);
    json["scenes"] = serde_json::json!([{"nodes":[0]}, {"nodes":[3]}]);
    json["nodes"] = serde_json::json!([
        {"name":"unused_spawn", "extras":{"mq_tag":"unused"}},
        {"name":"wall_collider", "mesh":0, "translation":[-2,0,0],
         "extras":{"mq_tag":"wall"}},
        {"name":"same_spawn", "translation":[1,0,0],
         "rotation":[0,0.38268343,0,0.9238795], "extras":{"mq_tag":"player"}},
        {"name":"parent", "translation":[0,1,0],
         "rotation":[0,0.25881905,0,0.9659258], "children":[1,4,2,5,6,7,8,9]},
        {"name":"same_spawn", "translation":[-1,0,0], "extras":{"mq_tag":"guard"}},
        {"name":"goal_trigger.001", "translation":[2,0,0],
         "scale":[0.4,0.4,0.4], "extras":{"mq_shape":"sphere","mq_tag":"goal"}},
        {"name":"post_collider", "translation":[0,0,2], "scale":[0.2,0.6,0.2],
         "extras":{"mq_shape":"capsule", "mq_tag":""}},
        {"name":"cover_collider", "translation":[0,0,-2],
         "extras":{"mq_shape":"box","mq_tag":"cover"}},
        {"name":"tag_only", "extras":{"mq_tag":"label"}},
        {"name":"collapsed_spawn", "scale":[0,1,1], "extras":{"mq_tag":7}}
    ]);
    gltf::binary::Glb {
        header: source.header,
        json: Cow::Owned(serde_json::to_vec(&json).unwrap()),
        bin: source.bin,
    }
    .to_vec()
    .unwrap()
}

/// The CPU-only test version samples no image and needs no graphics context.
#[cfg(test)]
pub fn untextured_bytes() -> Vec<u8> {
    let bytes = bytes();
    let source = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&source.json).unwrap();
    json["materials"][0]["pbrMetallicRoughness"]
        .as_object_mut()
        .unwrap()
        .remove("baseColorTexture");
    gltf::binary::Glb {
        header: source.header,
        json: Cow::Owned(serde_json::to_vec(&json).unwrap()),
        bin: source.bin,
    }
    .to_vec()
    .unwrap()
}
