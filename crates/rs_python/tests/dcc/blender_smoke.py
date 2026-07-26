"""Blender smoke test. Targets Blender 4.x only.

`MeshPolygon.loop_total` is read-only from Blender 4.0 onward — polygons reference a
slice of the loop array, and each polygon's loop count is derived from `loop_start`
together with polygon order, not settable directly. This script relies on that 4.x
behaviour: for a uniform triangle list, setting only `loop_start` (evenly spaced by 3)
is sufficient because loops and polygons are kept in the same order, so Blender infers
`loop_total = 3` for every polygon from the gap to the next `loop_start`. On Blender
3.x, `loop_total` still needs to be set explicitly; this script does not do that.
"""

import sys

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
mesh.update()

assert len(mesh.vertices) == skn.vertex_count, "vertex count mismatch"
assert len(mesh.polygons) == corner_count // 3, "triangle count mismatch"
print(f"OK blender vertices={len(mesh.vertices)} triangles={len(mesh.polygons)}")
