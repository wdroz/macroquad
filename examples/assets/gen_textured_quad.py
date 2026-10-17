"""Generate the tiny, original checkerboard GLB used by gltf_textured.rs.

Uses only the Python standard library; run from any directory.
"""
import json
from pathlib import Path
import struct
import zlib


def png_chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


colors = [(245, 177, 64, 255), (42, 161, 188, 255)]
rows = b"".join(b"\0" + b"".join(bytes(colors[(x + y) % 2]) for x in range(4)) for y in range(4))
png = b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", struct.pack(">2I5B", 4, 4, 8, 6, 0, 0, 0)) + png_chunk(b"IDAT", zlib.compress(rows)) + png_chunk(b"IEND", b"")
positions = struct.pack("<12f", -1, -1, 0, 1, -1, 0, 1, 1, 0, -1, 1, 0)
normals = struct.pack("<12f", *([0, 0, 1] * 4))
uvs = struct.pack("<8f", 0, 1, 1, 1, 1, 0, 0, 0)
indices = struct.pack("<6H", 0, 1, 2, 0, 2, 3)
binary = positions + normals + uvs + indices + png
json_data = {
    "asset": {"version": "2.0", "generator": "macroquad gen_textured_quad.py"},
    "scene": 0, "scenes": [{"nodes": [0]}],
    "nodes": [{"name": "checkerboard", "mesh": 0}],
    "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2}, "indices": 3, "material": 0}]}],
    "materials": [{"doubleSided": True, "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicFactor": 0}}],
    "textures": [{"source": 0, "sampler": 0}],
    "samplers": [{"magFilter": 9728, "minFilter": 9728}],
    "images": [{"bufferView": 4, "mimeType": "image/png"}],
    "buffers": [{"byteLength": len(binary)}],
    "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 48}, {"buffer": 0, "byteOffset": 48, "byteLength": 48}, {"buffer": 0, "byteOffset": 96, "byteLength": 32}, {"buffer": 0, "byteOffset": 128, "byteLength": 12}, {"buffer": 0, "byteOffset": 140, "byteLength": len(png)}],
    "accessors": [{"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [-1, -1, 0], "max": [1, 1, 0]}, {"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3"}, {"bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC2"}, {"bufferView": 3, "componentType": 5123, "count": 6, "type": "SCALAR"}],
}
document = json.dumps(json_data, separators=(",", ":")).encode()
document += b" " * (-len(document) % 4)
binary += b"\0" * (-len(binary) % 4)
output = struct.pack("<4sII", b"glTF", 2, 28 + len(document) + len(binary))
output += struct.pack("<I4s", len(document), b"JSON") + document
output += struct.pack("<I4s", len(binary), b"BIN\0") + binary
Path(__file__).with_name("textured_quad.glb").write_bytes(output)
