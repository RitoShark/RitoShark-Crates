from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
SKL = FIXTURES / "aatrox.skl"

fixture_only = pytest.mark.skipif(not SKL.exists(), reason="fixture not present")


@fixture_only
def test_read_joints():
    skl = ritoshark.Skl.from_path(str(SKL))
    assert len(skl.joints) > 0
    j = skl.joints[0]
    assert isinstance(j.name, str)
    assert len(j.local_translation) == 3
    assert len(j.local_scale) == 3
    assert len(j.local_rotation) == 4
    assert len(j.inverse_bind_translation) == 3
    assert isinstance(j.parent_id, int)


@fixture_only
def test_root_joint_has_no_parent():
    skl = ritoshark.Skl.from_path(str(SKL))
    assert any(j.parent_id < 0 for j in skl.joints)


@fixture_only
def test_reimport_equivalence():
    src = ritoshark.Skl.from_path(str(SKL))
    rebuilt = ritoshark.Skl.new(
        joints=[
            ritoshark.Joint(
                name=j.name,
                id=j.id,
                parent_id=j.parent_id,
                radius=j.radius,
                local_translation=j.local_translation,
                local_scale=j.local_scale,
                local_rotation=j.local_rotation,
                inverse_bind_translation=j.inverse_bind_translation,
                inverse_bind_scale=j.inverse_bind_scale,
                inverse_bind_rotation=j.inverse_bind_rotation,
            )
            for j in src.joints
        ],
        influences=list(src.influences),
        name=src.name,
        asset=src.asset,
    )
    back = ritoshark.Skl.from_bytes(rebuilt.to_bytes())
    assert [j.name for j in back.joints] == [j.name for j in src.joints]
    assert [j.parent_id for j in back.joints] == [j.parent_id for j in src.joints]
    assert list(back.influences) == list(src.influences)


def test_joint_hash_is_computed_from_name():
    j = ritoshark.Joint(
        name="root",
        id=0,
        parent_id=-1,
        radius=0.0,
        local_translation=(0.0, 0.0, 0.0),
        local_scale=(1.0, 1.0, 1.0),
        local_rotation=(0.0, 0.0, 0.0, 1.0),
        inverse_bind_translation=(0.0, 0.0, 0.0),
        inverse_bind_scale=(1.0, 1.0, 1.0),
        inverse_bind_rotation=(0.0, 0.0, 0.0, 1.0),
    )
    assert j.hash != 0


def test_joint_hash_matches_known_vector():
    lower = ritoshark.Joint(
        name="root",
        id=0,
        parent_id=-1,
        radius=0.0,
        local_translation=(0.0, 0.0, 0.0),
        local_scale=(1.0, 1.0, 1.0),
        local_rotation=(0.0, 0.0, 0.0, 1.0),
        inverse_bind_translation=(0.0, 0.0, 0.0),
        inverse_bind_scale=(1.0, 1.0, 1.0),
        inverse_bind_rotation=(0.0, 0.0, 0.0, 1.0),
    )
    assert lower.hash == 0x00079664

    mixed = ritoshark.Joint(
        name="Root",
        id=0,
        parent_id=-1,
        radius=0.0,
        local_translation=(0.0, 0.0, 0.0),
        local_scale=(1.0, 1.0, 1.0),
        local_rotation=(0.0, 0.0, 0.0, 1.0),
        inverse_bind_translation=(0.0, 0.0, 0.0),
        inverse_bind_scale=(1.0, 1.0, 1.0),
        inverse_bind_rotation=(0.0, 0.0, 0.0, 1.0),
    )
    assert mixed.hash == lower.hash


def test_inmemory_roundtrip():
    root = ritoshark.Joint(
        name="root",
        id=0,
        parent_id=-1,
        radius=1.0,
        local_translation=(0.0, 0.0, 0.0),
        local_scale=(1.0, 1.0, 1.0),
        local_rotation=(0.0, 0.0, 0.0, 1.0),
        inverse_bind_translation=(0.0, 0.0, 0.0),
        inverse_bind_scale=(1.0, 1.0, 1.0),
        inverse_bind_rotation=(0.0, 0.0, 0.0, 1.0),
    )
    hip = ritoshark.Joint(
        name="hip",
        id=1,
        parent_id=0,
        radius=1.0,
        local_translation=(0.0, 1.0, 0.0),
        local_scale=(1.0, 1.0, 1.0),
        local_rotation=(0.0, 0.0, 0.0, 1.0),
        inverse_bind_translation=(0.0, -1.0, 0.0),
        inverse_bind_scale=(1.0, 1.0, 1.0),
        inverse_bind_rotation=(0.0, 0.0, 0.0, 1.0),
    )
    spine = ritoshark.Joint(
        name="spine",
        id=2,
        parent_id=1,
        radius=1.0,
        local_translation=(0.0, 2.0, 0.0),
        local_scale=(1.0, 1.0, 1.0),
        local_rotation=(0.0, 0.0, 0.0, 1.0),
        inverse_bind_translation=(0.0, -3.0, 0.0),
        inverse_bind_scale=(1.0, 1.0, 1.0),
        inverse_bind_rotation=(0.0, 0.0, 0.0, 1.0),
    )
    joints = [root, hip, spine]

    skl = ritoshark.Skl.new(
        joints=joints,
        influences=[0, 1, 2],
        name="test_rig",
        asset="test_rig.skl",
    )
    back = ritoshark.Skl.from_bytes(skl.to_bytes())

    assert [j.name for j in back.joints] == [j.name for j in joints]
    assert [j.id for j in back.joints] == [j.id for j in joints]
    assert [j.parent_id for j in back.joints] == [j.parent_id for j in joints]
    assert [j.hash for j in back.joints] == [j.hash for j in joints]
    assert list(back.influences) == [0, 1, 2]
    assert back.name == "test_rig"
    assert back.asset == "test_rig.skl"


def test_parse_error_on_garbage():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.Skl.from_bytes(b"definitely not a skeleton")
