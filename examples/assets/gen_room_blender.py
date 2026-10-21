"""Regenerate the original tiny room (Blender 4.5+).

blender --background --factory-startup --python examples/assets/gen_room_blender.py
Optional output path follows --. Normal builds/tests use the committed GLB.
This resets the scene; run in a separate headless Blender process.
"""
import sys
from pathlib import Path
import bpy


def main():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    material = bpy.data.materials.new("Room blue")
    material.diffuse_color = (0.18, 0.38, 0.58, 1.0)
    # Blender Z-up positions and full dimensions. No scale is applied: both
    # size-1 meshes and box empties have exactly these full dimensions.
    boxes = [
        ("floor", (0, 0, -0.1), (8, 8, 0.2)),
        ("left", (-4, 0, 1), (0.2, 8, 2)),
        ("right", (4, 0, 1), (0.2, 8, 2)),
        ("back", (0, 4, 1), (8, 0.2, 2)),
    ]
    for tag, position, size in boxes:
        bpy.ops.mesh.primitive_cube_add(size=1, location=position)
        visual = bpy.context.object
        visual.name = tag + "_visual"
        visual.scale = size
        visual.data.materials.append(material)
        marker = bpy.data.objects.new(tag + "_collider", None)
        bpy.context.collection.objects.link(marker)
        marker.location, marker.scale = position, size
        marker["mq_role"], marker["mq_shape"], marker["mq_tag"] = "collider", "box", tag
    for name, role, tag, position, scale, shape in [
        ("goal_trigger.001", "trigger", "goal", (0, 2, 0.4), (0.6, 0.6, 0.6), "sphere"),
        ("player_spawn.001", "spawn", "player", (0, -2, 0.4), (1, 1, 1), None),
    ]:
        marker = bpy.data.objects.new(name, None)
        bpy.context.collection.objects.link(marker)
        marker.location, marker.scale = position, scale
        marker["mq_role"], marker["mq_tag"] = role, tag
        if shape:
            marker["mq_shape"] = shape
    path = Path(__file__).resolve().with_name("room.glb")
    if "--" in sys.argv:
        path = Path(sys.argv[sys.argv.index("--") + 1]).resolve()
    bpy.ops.export_scene.gltf(filepath=str(path), export_format="GLB",
                              export_extras=True, export_yup=True,
                              export_animations=False, export_cameras=False,
                              export_lights=False)
    print("ROOM_EXPORTED", path)


if __name__ == "__main__":
    main()
