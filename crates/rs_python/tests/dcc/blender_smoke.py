import sys
from pathlib import Path

import bpy

import ritoshark

skn_path = sys.argv[-1]
skn = ritoshark.Skn.from_path(skn_path)

mesh = bpy.data.meshes.new("smoke")
mesh.vertices.add(skn.vertex_count)
mesh.vertices.foreach_set("co", memoryview(skn.positions).cast("f"))

corner_count = len(skn.indices) // 4
mesh.loops.add(corner_count)
mesh.loops.foreach_set("vertex_index", memoryview(skn.indices).cast("I"))
mesh.polygons.add(corner_count // 3)
mesh.polygons.foreach_set("loop_start", range(0, corner_count, 3))
mesh.polygons.foreach_set("loop_total", [3] * (corner_count // 3))
mesh.update()

assert len(mesh.vertices) == skn.vertex_count, "vertex count mismatch"
assert len(mesh.polygons) == corner_count // 3, "triangle count mismatch"
print(f"OK blender vertices={len(mesh.vertices)} triangles={len(mesh.polygons)}")
