import struct
import sys

import maya.standalone

maya.standalone.initialize()

import maya.cmds as cmds  # noqa: E402
from maya.api.OpenMaya import MFloatPointArray, MFnMesh, MIntArray  # noqa: E402

import ritoshark  # noqa: E402

skn_path = sys.argv[-1]
skn = ritoshark.Skn.from_path(skn_path)

floats = struct.unpack(f"<{skn.vertex_count * 3}f", skn.positions)
points = MFloatPointArray()
for i in range(skn.vertex_count):
    points.append((floats[i * 3], floats[i * 3 + 1], floats[i * 3 + 2], 1.0))

corner_count = len(skn.indices) // 4
indices = struct.unpack(f"<{corner_count}I", skn.indices)
face_counts = MIntArray()
face_connects = MIntArray()
for i in range(0, corner_count, 3):
    face_counts.append(3)
    face_connects.append(indices[i])
    face_connects.append(indices[i + 1])
    face_connects.append(indices[i + 2])

MFnMesh().create(points, face_counts, face_connects)

meshes = cmds.ls(type="mesh")
assert meshes, "no mesh created"
count = cmds.polyEvaluate(meshes[0], vertex=True)
assert count == skn.vertex_count, f"expected {skn.vertex_count} vertices, got {count}"
print(f"OK maya vertices={count}")
