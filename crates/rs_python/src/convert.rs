/*!
Packs geometry into tightly packed little-endian buffers and unpacks the same layout back. Bulk
data crosses the Python boundary as one buffer per attribute with no padding or interleaving, so
Blender's `foreach_set` and Maya's array types consume it without a per-element loop.
*/

use pyo3::prelude::*;
use ritoshark::math::{Vec2, Vec3};

use crate::error::write_err;

pub fn pack_vec3(items: impl Iterator<Item = Vec3>) -> Vec<u8> {
    let mut out = Vec::new();
    for v in items {
        out.extend_from_slice(&v.x.to_le_bytes());
        out.extend_from_slice(&v.y.to_le_bytes());
        out.extend_from_slice(&v.z.to_le_bytes());
    }
    out
}

pub fn pack_vec2(items: impl Iterator<Item = Vec2>) -> Vec<u8> {
    let mut out = Vec::new();
    for v in items {
        out.extend_from_slice(&v.x.to_le_bytes());
        out.extend_from_slice(&v.y.to_le_bytes());
    }
    out
}

pub fn pack_f32(items: impl Iterator<Item = f32>) -> Vec<u8> {
    let mut out = Vec::new();
    for v in items {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

pub fn pack_u32(items: impl Iterator<Item = u32>) -> Vec<u8> {
    let mut out = Vec::new();
    for v in items {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn chunks_of(bytes: &[u8], size: usize, label: &str) -> PyResult<usize> {
    if bytes.len() % size != 0 {
        return Err(write_err(format!(
            "{label}: buffer length {} is not a multiple of {size}",
            bytes.len()
        )));
    }
    Ok(bytes.len() / size)
}

pub fn unpack_f32(bytes: &[u8], label: &str) -> PyResult<Vec<f32>> {
    let count = chunks_of(bytes, 4, label)?;
    Ok((0..count)
        .map(|i| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
        .collect())
}

pub fn unpack_u32(bytes: &[u8], label: &str) -> PyResult<Vec<u32>> {
    let count = chunks_of(bytes, 4, label)?;
    Ok((0..count)
        .map(|i| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
        .collect())
}

pub fn unpack_vec3(bytes: &[u8], label: &str) -> PyResult<Vec<Vec3>> {
    let count = chunks_of(bytes, 12, label)?;
    let f = unpack_f32(bytes, label)?;
    Ok((0..count)
        .map(|i| Vec3::new(f[i * 3], f[i * 3 + 1], f[i * 3 + 2]))
        .collect())
}

pub fn unpack_vec2(bytes: &[u8], label: &str) -> PyResult<Vec<Vec2>> {
    let count = chunks_of(bytes, 8, label)?;
    let f = unpack_f32(bytes, label)?;
    Ok((0..count)
        .map(|i| Vec2::new(f[i * 2], f[i * 2 + 1]))
        .collect())
}

pub fn expect_count(actual: usize, expected: usize, label: &str) -> PyResult<()> {
    if actual != expected {
        return Err(write_err(format!(
            "{label}: expected {expected} elements, got {actual}"
        )));
    }
    Ok(())
}

#[pyfunction]
fn _pack_vec3_test(items: Vec<(f32, f32, f32)>) -> Vec<u8> {
    pack_vec3(items.into_iter().map(|(x, y, z)| Vec3::new(x, y, z)))
}

#[pyfunction]
fn _unpack_vec3_test(bytes: Vec<u8>) -> PyResult<usize> {
    unpack_vec3(&bytes, "positions").map(|v| v.len())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(_pack_vec3_test, m)?)?;
    m.add_function(wrap_pyfunction!(_unpack_vec3_test, m)?)?;
    Ok(())
}
