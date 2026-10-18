"""Build the Marble Maze level and export ``marble_maze.glb``.

This is the scene an AI Blender MCP server would create: cubes for the
floor and walls, empties for the spawn and the goal, then a glTF 2.0
export. Run it with headless Blender, from the repository root or from
anywhere:

    blender --background --python examples/assets/gen_marble_maze.py

Blender is Z-up. The exporter's ``+Y Up`` option writes glTF Y-up, which
is what the game loads. Cubes are ``size=1`` (vertices at ±0.5) so the
node scale is the full size, matching ``Aabb::from_center_size`` and
``Obb::from_world_matrix``. A default Blender cube is size 2 and would
not.
"""

import math
import os

import bpy

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
OUTPUT = os.path.join(SCRIPT_DIR, "marble_maze.glb")


def clear_scene():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for block in (
        bpy.data.meshes,
        bpy.data.materials,
        bpy.data.cameras,
        bpy.data.lights,
    ):
        for data in list(block):
            if data.users == 0:
                block.remove(data)


def colorize(obj, rgba):
    material = bpy.data.materials.new(name=obj.name + "_mat")
    material.use_nodes = True
    principled = material.node_tree.nodes.get("Principled BSDF")
    principled.inputs["Base Color"].default_value = rgba
    principled.inputs["Roughness"].default_value = 1.0
    principled.inputs["Metallic"].default_value = 0.0
    obj.data.materials.clear()
    obj.data.materials.append(material)


def add_cube(name, location, scale, rgba, yaw_degrees=0.0, linked_from=None):
    """A collider cube. ``scale`` is the full size, because the mesh spans ±0.5."""
    if linked_from is None:
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=location)
        obj = bpy.context.active_object
    else:
        obj = linked_from.copy()
        obj.data = linked_from.data
        bpy.context.scene.collection.objects.link(obj)
        obj.location = location
    obj.name = name
    obj.scale = scale
    obj.rotation_euler = (0.0, 0.0, math.radians(yaw_degrees))
    if linked_from is None:
        colorize(obj, rgba)
    return obj


def add_empty(name, location, scale, props):
    bpy.ops.object.empty_add(type="PLAIN_AXES", location=location)
    obj = bpy.context.active_object
    obj.name = name
    obj.scale = scale
    obj.empty_display_size = 0.4
    for key, value in props.items():
        obj[key] = value
    return obj


def build():
    clear_scene()
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.scale_length = 1.0

    floor_color = (0.36, 0.58, 0.40, 1.0)
    wall_color = (0.75, 0.58, 0.40, 1.0)
    bumper_color = (0.42, 0.28, 0.24, 1.0)

    # Blender is Z-up here. +Y Up on export maps (x, y, z) to glTF (x, z, -y).
    # Floor top is z = 0. Walls are 1 m tall and 0.4 m thick. The playable
    # yard is about 8.6 m across, with two baffles and one diagonal bumper.
    add_cube("floor_collider", (0.0, 0.0, -0.1), (10.0, 10.0, 0.2), floor_color)

    west = add_cube("west_collider", (-4.5, 0.0, 0.5), (0.4, 9.4, 1.0), wall_color)
    add_cube("east_collider", (4.5, 0.0, 0.5), (0.4, 9.4, 1.0), wall_color, linked_from=west)
    add_cube("south_collider", (0.0, -4.5, 0.5), (9.4, 0.4, 1.0), wall_color, linked_from=west)
    add_cube("north_collider", (0.0, 4.5, 0.5), (9.4, 0.4, 1.0), wall_color, linked_from=west)
    # Gap on Blender +X (east). After export that gap is still toward glTF +X.
    add_cube("baffle_east_gap_collider", (-1.2, 1.2, 0.5), (6.0, 0.4, 1.0), wall_color, linked_from=west)
    # Gap on Blender -X (west).
    add_cube("baffle_west_gap_collider", (1.2, -1.2, 0.5), (6.0, 0.4, 1.0), wall_color, linked_from=west)

    bumper = add_cube(
        "bumper_collider",
        (3.6, 1.2, 0.5),
        (1.0, 0.3, 1.0),
        bumper_color,
        yaw_degrees=45.0,
    )
    bumper.data = bumper.data.copy()
    colorize(bumper, bumper_color)

    # Sphere radius is the largest scale component. Spawn is the marble center.
    add_empty(
        "goal_trigger",
        (-2.5, 2.8, 0.45),
        (0.65, 0.65, 0.65),
        {"mq_shape": "sphere", "mq_role": "trigger"},
    )
    add_empty("player_spawn", (0.0, -3.0, 0.35), (1.0, 1.0, 1.0), {})


def export():
    bpy.ops.export_scene.gltf(
        filepath=OUTPUT,
        export_format="GLB",
        export_extras=True,
        export_yup=True,
        export_apply=False,
        export_animations=False,
        export_skins=False,
        export_morph=False,
        export_lights=False,
        export_cameras=False,
    )
    print(f"wrote {OUTPUT}")


if __name__ == "__main__":
    build()
    export()
