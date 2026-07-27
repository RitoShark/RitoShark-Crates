"""Blender smoke test. Targets Blender 4.x only.

`MeshPolygon.loop_total` is read-only from Blender 4.0 onward (confirmed: Blender's own
`bpy.types.MeshPolygon` API docs describe it with no setter, consistently across
versions, and this matches the exact `AttributeError` a real 4.x run raised against an
earlier version of this script). So this script sets only `loop_start` (evenly spaced
by 3) and does not touch `loop_total` at all.

What is NOT execution-verified, only inferred from documentation: whether Blender
actually derives every polygon's loop count correctly from a `loop_start`-only array.
In particular, Blender's internal face-offset representation is understood to use N+1
offset entries for N faces, the last being a sentinel equal to the total corner count;
whether `mesh.update()`/`mesh.validate()` reconstructs that sentinel correctly from a
polygons array that only carries `loop_start` is not confirmed by any source consulted
for this script. If it does not, the LAST triangle specifically is the one at risk of
being malformed or dropped — a mesh that looks almost right, not one that crashes. The
polygon-count and loop-count assertions below exist specifically to catch that failure
mode loudly instead of printing OK on a subtly wrong mesh. There is no Blender install
on the machine this script was authored on, so none of this has been run; treat the
first real run on Blender 4.x as the actual test of this design, not this script's mere
existence.

On Blender 3.x, `loop_total` still needs to be set explicitly; this script does not do
that and will not work there. No version-branching is implemented on purpose — 4.x is
the only target.
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
triangle_count = corner_count // 3
mesh.loops.add(corner_count)
mesh.loops.foreach_set("vertex_index", memoryview(skn.indices).cast("I"))
mesh.polygons.add(triangle_count)
mesh.polygons.foreach_set("loop_start", range(0, corner_count, 3))
mesh.update()
mesh.validate(verbose=True)

assert len(mesh.vertices) == skn.vertex_count, "vertex count mismatch"
assert len(mesh.polygons) == triangle_count, "triangle count mismatch"
assert len(mesh.loops) == corner_count, "loop count mismatch (last polygon may be malformed)"
loop_totals = [0] * triangle_count
mesh.polygons.foreach_get("loop_total", loop_totals)
assert loop_totals == [3] * triangle_count, (
    f"loop_total derivation is wrong for at least one polygon (likely the last): {loop_totals}"
)
print(f"OK blender vertices={len(mesh.vertices)} triangles={len(mesh.polygons)}")
