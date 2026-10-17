#!/usr/bin/env python3
"""Write examples/assets/triangle.gltf and triangle.glb.

The scene is a parent, a scaled mesh child, a rotated child, a second node
sharing that mesh, a matrix node, a box-collider empty, a spawn empty, and one
node left out of the scene. The collider is marked with node extras
(`mq_shape`, `mq_role`). The spawn is marked with a `_spawn` name suffix.
Regenerate with this script. Tests load the committed bytes and do not run
Python.
"""

import base64
import json
import math
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def f32s(*values):
    return struct.pack("<" + "f" * len(values), *values)


def build_buffer():
    positions = f32s(0, 0, 0, 1, 0, 0, 0, 1, 0)
    normals = f32s(0, 0, 1, 0, 0, 1, 0, 0, 1)
    uvs = f32s(0, 0, 1, 0, 0, 1)
    indices = struct.pack("<HHH", 0, 1, 2)
    positions2 = f32s(0, 0, 0, 0, 0, 1, 0, 1, 0)
    # Pad the 6-byte index view so the next float view stays 4-byte aligned.
    return positions + normals + uvs + indices + b"\x00\x00" + positions2


def document(buffer_uri, byte_length):
    half = math.sqrt(2.0) / 2.0
    # Column-major translation by (4, 5, 6).
    marker = [
        1, 0, 0, 0,
        0, 1, 0, 0,
        0, 0, 1, 0,
        4, 5, 6, 1,
    ]
    return {
        "asset": {"version": "2.0", "generator": "macroquad examples/assets/gen_triangle_gltf.py"},
        "scene": 0,
        "scenes": [{"name": "sample", "nodes": [0, 4, 7]}],
        "nodes": [
            {"name": "root", "translation": [1, 2, 3], "children": [1, 2, 3, 6]},
            {"name": "box", "translation": [0, 1, 0], "scale": [2, 2, 2], "mesh": 0},
            {"name": "spin", "rotation": [0, half, 0, half]},
            {"name": "box_copy", "mesh": 0},
            {"name": "marker", "matrix": marker},
            {"name": "hidden", "translation": [9, 9, 9]},
            {
                "name": "crate_collider",
                "translation": [1, 0, 0],
                "scale": [2, 1, 0.5],
                "extras": {"mq_shape": "box", "mq_role": "collider"},
            },
            {"name": "player_spawn", "translation": [8, 0, 1]},
        ],
        "meshes": [
            {
                "name": "tri",
                "primitives": [
                    {
                        "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
                        "indices": 3,
                        "material": 0,
                    },
                    {"attributes": {"POSITION": 4}, "material": 1},
                ],
            }
        ],
        "materials": [
            {"name": "blue", "pbrMetallicRoughness": {"baseColorFactor": [0.2, 0.4, 0.6, 1]}},
            {"name": "red", "pbrMetallicRoughness": {"baseColorFactor": [1, 0, 0, 0.5]}},
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]},
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2"},
            {"bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR"},
            {"bufferView": 4, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [0, 1, 1]},
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 36},
            {"buffer": 0, "byteOffset": 72, "byteLength": 24},
            {"buffer": 0, "byteOffset": 96, "byteLength": 6},
            {"buffer": 0, "byteOffset": 104, "byteLength": 36},
        ],
        "buffers": [{"byteLength": byte_length, **({"uri": buffer_uri} if buffer_uri else {})}],
    }


def pad(data, alignment, byte):
    data = bytearray(data)
    while len(data) % alignment:
        data.append(byte)
    return bytes(data)


def main():
    blob = build_buffer()
    encoded = base64.b64encode(blob).decode("ascii")
    uri = "data:application/octet-stream;base64," + encoded
    gltf = document(uri, len(blob))
    (ROOT / "triangle.gltf").write_text(json.dumps(gltf, indent=2) + "\n", encoding="utf-8")

    glb_json = json.dumps(document(None, len(blob)), separators=(",", ":")).encode("utf-8")
    json_chunk = pad(glb_json, 4, 0x20)
    bin_chunk = pad(blob, 4, 0x00)
    total = 12 + 8 + len(json_chunk) + 8 + len(bin_chunk)
    header = struct.pack("<4sII", b"glTF", 2, total)
    chunks = b"".join(
        (
            struct.pack("<I4s", len(json_chunk), b"JSON"),
            json_chunk,
            struct.pack("<I4s", len(bin_chunk), b"BIN\0"),
            bin_chunk,
        )
    )
    (ROOT / "triangle.glb").write_bytes(header + chunks)
    print(f"wrote triangle.gltf and triangle.glb ({len(blob)} buffer bytes)")


if __name__ == "__main__":
    main()
