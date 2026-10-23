//! Original tiny quad with one child-transform and weight clip, no assets.

use serde_json::{json, Value};

pub fn document() -> (Value, Vec<u8>) {
    let mut binary = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    for (values, width, kind) in [
        (
            vec![
                -0.14_f32, -0.14, 0., 0.14, -0.14, 0., 0.14, 0.14, 0., -0.14, 0.14, 0.,
            ],
            3,
            "VEC3",
        ),
        (
            vec![0.2, 0., 0., 0.2, 0., 0., 0.2, 0., 0., 0.2, 0., 0.],
            3,
            "VEC3",
        ),
        (vec![0., 2.], 1, "SCALAR"),
        (vec![-0.5, 0., 0., 0.3, 0., 0.], 3, "VEC3"),
        (
            vec![
                0.,
                0.,
                0.,
                1.,
                0.,
                0.,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ],
            4,
            "VEC4",
        ),
        (vec![1., 1., 1., 1., 1.5, 1.], 3, "VEC3"),
        (vec![0., 1.], 1, "SCALAR"),
    ] {
        let offset = binary.len();
        binary.extend(values.iter().flat_map(|value| value.to_le_bytes()));
        let view = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":values.len()*4}));
        let mut accessor =
            json!({"bufferView":view,"componentType":5126,"count":values.len()/width,"type":kind});
        if view == 0 {
            accessor["min"] = json!([-0.14, -0.14, 0.]);
            accessor["max"] = json!([0.14, 0.14, 0.]);
        }
        if view == 2 {
            accessor["min"] = json!([0.]);
            accessor["max"] = json!([2.]);
        }
        accessors.push(accessor);
    }
    let offset = binary.len();
    binary.extend(
        [0_u16, 1, 2, 0, 2, 3]
            .into_iter()
            .flat_map(u16::to_le_bytes),
    );
    views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":12}));
    accessors.push(json!({"bufferView":7,"componentType":5123,"count":6,"type":"SCALAR"}));
    let doc = json!({
        "asset":{"version":"2.0"}, "scene":0,"scenes":[{"nodes":[2]}],
        "nodes":[{"name":"quad","mesh":0,"translation":[-0.5,0.,0.]}, {"name":"outside"}, {"name":"parent","children":[0],"translation":[0.,0.2,0.]}],
        "buffers":[{"byteLength":binary.len()}], "bufferViews":views,"accessors":accessors,
        "meshes":[{"weights":[0.],"primitives":[{"attributes":{"POSITION":0},"indices":7,"targets":[{"POSITION":1}]}]}],
        "animations":[{"name":"ChildAndWeight","samplers":[
            {"input":2,"output":3,"interpolation":"LINEAR"}, {"input":2,"output":4,"interpolation":"LINEAR"},
            {"input":2,"output":5,"interpolation":"STEP"}, {"input":2,"output":6,"interpolation":"LINEAR"}
        ],"channels":[
            {"sampler":0,"target":{"node":0,"path":"translation"}}, {"sampler":1,"target":{"node":0,"path":"rotation"}},
            {"sampler":2,"target":{"node":0,"path":"scale"}}, {"sampler":3,"target":{"node":0,"path":"weights"}}
        ]}]
    });
    (doc, binary)
}

pub fn pack(mut doc: Value, mut binary: Vec<u8>) -> Vec<u8> {
    doc["buffers"][0]["byteLength"] = json!(binary.len());
    binary.resize((binary.len() + 3) & !3, 0);
    let mut json = serde_json::to_vec(&doc).unwrap();
    json.resize((json.len() + 3) & !3, b' ');
    let mut bytes = Vec::new();
    bytes.extend(b"glTF");
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend(((28 + json.len() + binary.len()) as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(json);
    bytes.extend((binary.len() as u32).to_le_bytes());
    bytes.extend(b"BIN\0");
    bytes.extend(binary);
    bytes
}

pub fn glb() -> Vec<u8> {
    let (doc, binary) = document();
    pack(doc, binary)
}
