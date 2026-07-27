/*!
Reads `.bin` documents into plain Python values. This view is deliberately lossy: it drops the
hash-versus-resolved-name duality, the LIST/LIST2 distinction, and duplicate map keys, none of
which a reader needs. Writing bins requires an editable tree that preserves every round-trip
invariant, which is what `read_bin_editable`/`write_bin` provide alongside it.

The editable tree mirrors upstream shape directly: `entries` is a list (upstream `Vec<BinEntry>`,
so duplicate entry hashes are representable), each entry's `fields` is a dict keyed by raw `u32`
hash (upstream `IndexMap`, so order and uniqueness both hold for free), and containers are tagged
dicts (`list`/`list2`/`pointer`/`embed`/`map`/`option`) carrying their declared element type so a
struct field with no declaring container can still be written back to its exact original type.
`read_bin_editable` emits the explicit `{"__type__": ..., "value": ...}` form for any primitive
where bare Python inference (`int -> I32`, `float -> F32`, `str -> String`, `bool -> Bool`,
`None -> None`) would not reproduce the original `BinType`, and always for the fixed-arity tuple
types (`Vec2`/`Vec3`/`Vec4`/`Mtx44`/`Rgba`), since a bare tuple has no inference rule at all. This
is what makes an unmodified read -> write byte-identical without the caller reasoning about types.

`write_bin`/`write_bin_bytes` validate the whole document before emitting any bytes and raise
`WriteError` carrying the path to the offending value (e.g.
`entries[3].fields[0xe095d841].items[2]: expected str for item type 'string', got int`), since a
panic deep in a 3,000-line document is undebuggable and a bare message without a path is not much
better.

Both directions respect `MAX_DEPTH` so a pathological document can only ever raise, never overflow
the native stack.
*/

use indexmap::IndexMap;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyList, PyString, PyTuple};
use ritoshark::bin::{Bin, BinEntry, BinPatch, BinType, BinValue};
use ritoshark::prelude::{Parse, Serialize};

use crate::error::{parse_err, write_err};

const MAX_DEPTH: usize = 128;

fn value_to_py<'py>(py: Python<'py>, v: &BinValue, depth: usize) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(parse_err(format!(
            "bin value nesting exceeds maximum depth of {MAX_DEPTH}"
        )));
    }
    Ok(match v {
        BinValue::None => py.None().into_bound(py),
        BinValue::Bool(b) | BinValue::Flag(b) => b.into_pyobject(py)?.to_owned().into_any(),
        BinValue::I8(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U8(n) => n.into_pyobject(py)?.into_any(),
        BinValue::I16(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U16(n) => n.into_pyobject(py)?.into_any(),
        BinValue::I32(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U32(n) => n.into_pyobject(py)?.into_any(),
        BinValue::I64(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U64(n) => n.into_pyobject(py)?.into_any(),
        BinValue::F32(n) => n.into_pyobject(py)?.into_any(),
        BinValue::Vec2(a) => (a[0], a[1]).into_pyobject(py)?.into_any(),
        BinValue::Vec3(a) => (a[0], a[1], a[2]).into_pyobject(py)?.into_any(),
        BinValue::Vec4(a) => (a[0], a[1], a[2], a[3]).into_pyobject(py)?.into_any(),
        BinValue::Mtx44(a) => a.to_vec().into_pyobject(py)?.into_any(),
        BinValue::Rgba(a) => (a[0], a[1], a[2], a[3]).into_pyobject(py)?.into_any(),
        BinValue::String(s) => s.into_pyobject(py)?.into_any(),
        BinValue::Hash(h) | BinValue::Link(h) => h.into_pyobject(py)?.into_any(),
        BinValue::File(h) => h.into_pyobject(py)?.into_any(),
        BinValue::List { items, .. } => {
            let list = PyList::empty(py);
            for item in items {
                list.append(value_to_py(py, item, depth + 1)?)?;
            }
            list.into_any()
        }
        BinValue::Map { entries, .. } => {
            let dict = PyDict::new(py);
            for (k, val) in entries {
                dict.set_item(
                    value_to_py(py, k, depth + 1)?,
                    value_to_py(py, val, depth + 1)?,
                )?;
            }
            dict.into_any()
        }
        BinValue::Pointer { class, fields } | BinValue::Embed { class, fields } => {
            let dict = PyDict::new(py);
            dict.set_item("__class__", class)?;
            for (name_hash, val) in fields {
                dict.set_item(name_hash, value_to_py(py, val, depth + 1)?)?;
            }
            dict.into_any()
        }
        BinValue::Option { value, .. } => match value {
            Some(inner) => value_to_py(py, inner, depth + 1)?,
            None => py.None().into_bound(py),
        },
    })
}

fn doc_to_py<'py>(py: Python<'py>, bin: &Bin) -> PyResult<Bound<'py, PyDict>> {
    let doc = PyDict::new(py);
    doc.set_item("version", bin.version)?;
    doc.set_item("is_patch", bin.is_patch)?;
    doc.set_item("linked", bin.linked.clone())?;

    let entries = PyDict::new(py);
    for entry in &bin.entries {
        let fields = PyDict::new(py);
        fields.set_item("__class__", entry.class_hash)?;
        for (name_hash, val) in &entry.fields {
            fields.set_item(name_hash, value_to_py(py, val, 0)?)?;
        }
        entries.set_item(entry.path_hash, fields)?;
    }
    doc.set_item("entries", entries)?;
    Ok(doc)
}

#[pyfunction]
fn read_bin(py: Python<'_>, path: std::path::PathBuf) -> PyResult<Bound<'_, PyDict>> {
    let bin = Bin::from_path(path).map_err(parse_err)?;
    doc_to_py(py, &bin)
}

#[pyfunction]
fn read_bin_bytes<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let bin = Bin::from_bytes(data).map_err(parse_err)?;
    doc_to_py(py, &bin)
}

fn type_name(ty: BinType) -> &'static str {
    match ty {
        BinType::None => "none",
        BinType::Bool => "bool",
        BinType::I8 => "i8",
        BinType::U8 => "u8",
        BinType::I16 => "i16",
        BinType::U16 => "u16",
        BinType::I32 => "i32",
        BinType::U32 => "u32",
        BinType::I64 => "i64",
        BinType::U64 => "u64",
        BinType::F32 => "f32",
        BinType::Vec2 => "vec2",
        BinType::Vec3 => "vec3",
        BinType::Vec4 => "vec4",
        BinType::Mtx44 => "mtx44",
        BinType::Rgba => "rgba",
        BinType::String => "string",
        BinType::Hash => "hash",
        BinType::File => "file",
        BinType::List => "list",
        BinType::List2 => "list2",
        BinType::Pointer => "pointer",
        BinType::Embed => "embed",
        BinType::Link => "link",
        BinType::Option => "option",
        BinType::Map => "map",
        BinType::Flag => "flag",
    }
}

fn type_from_name(name: &str) -> Option<BinType> {
    Some(match name {
        "none" => BinType::None,
        "bool" => BinType::Bool,
        "i8" => BinType::I8,
        "u8" => BinType::U8,
        "i16" => BinType::I16,
        "u16" => BinType::U16,
        "i32" => BinType::I32,
        "u32" => BinType::U32,
        "i64" => BinType::I64,
        "u64" => BinType::U64,
        "f32" => BinType::F32,
        "vec2" => BinType::Vec2,
        "vec3" => BinType::Vec3,
        "vec4" => BinType::Vec4,
        "mtx44" => BinType::Mtx44,
        "rgba" => BinType::Rgba,
        "string" => BinType::String,
        "hash" => BinType::Hash,
        "file" => BinType::File,
        "list" => BinType::List,
        "list2" => BinType::List2,
        "pointer" => BinType::Pointer,
        "embed" => BinType::Embed,
        "link" => BinType::Link,
        "option" => BinType::Option,
        "map" => BinType::Map,
        "flag" => BinType::Flag,
        _ => return None,
    })
}

/// True when a bare Python `int`/`float`/`str`/`bool`/`None` for this type reproduces the exact
/// original type under the inference rule (`int -> I32`, `float -> F32`, `str -> String`,
/// `bool -> Bool`, `None -> None`). Every other primitive, including the fixed-arity tuple types
/// (which have no bare-tuple inference rule at all), must always use the explicit form.
fn is_inferred(ty: BinType) -> bool {
    matches!(
        ty,
        BinType::I32 | BinType::F32 | BinType::String | BinType::Bool | BinType::None
    )
}

fn primitive_to_editable_py<'py>(py: Python<'py>, v: &BinValue) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        BinValue::None => py.None().into_bound(py),
        BinValue::Bool(b) => b.into_pyobject(py)?.to_owned().into_any(),
        BinValue::I8(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U8(n) => n.into_pyobject(py)?.into_any(),
        BinValue::I16(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U16(n) => n.into_pyobject(py)?.into_any(),
        BinValue::I32(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U32(n) => n.into_pyobject(py)?.into_any(),
        BinValue::I64(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U64(n) => n.into_pyobject(py)?.into_any(),
        BinValue::F32(n) => n.into_pyobject(py)?.into_any(),
        BinValue::Vec2(a) => (a[0], a[1]).into_pyobject(py)?.into_any(),
        BinValue::Vec3(a) => (a[0], a[1], a[2]).into_pyobject(py)?.into_any(),
        BinValue::Vec4(a) => (a[0], a[1], a[2], a[3]).into_pyobject(py)?.into_any(),
        BinValue::Mtx44(a) => a.to_vec().into_pyobject(py)?.into_any(),
        BinValue::Rgba(a) => (a[0], a[1], a[2], a[3]).into_pyobject(py)?.into_any(),
        BinValue::String(s) => s.into_pyobject(py)?.into_any(),
        BinValue::Hash(h) => h.into_pyobject(py)?.into_any(),
        BinValue::Link(h) => h.into_pyobject(py)?.into_any(),
        BinValue::File(h) => h.into_pyobject(py)?.into_any(),
        BinValue::Flag(b) => b.into_pyobject(py)?.to_owned().into_any(),
        _ => {
            return Err(write_err(
                "primitive_to_editable_py called on a container value",
            ));
        }
    })
}

fn tagged_primitive<'py>(py: Python<'py>, v: &BinValue) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("__type__", type_name(v.ty()))?;
    dict.set_item("value", primitive_to_editable_py(py, v)?)?;
    Ok(dict.into_any())
}

/// Encodes a value that sits directly under a container declaring its element type (list item,
/// option item, map key/value): since the declared type already pins the primitive's type on
/// write, no explicit wrapping is needed regardless of inference.
fn value_to_editable_py_declared<'py>(
    py: Python<'py>,
    v: &BinValue,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(parse_err(format!(
            "bin value nesting exceeds maximum depth of {MAX_DEPTH}"
        )));
    }
    match v {
        BinValue::List { .. }
        | BinValue::Map { .. }
        | BinValue::Pointer { .. }
        | BinValue::Embed { .. }
        | BinValue::Option { .. } => value_to_editable_py(py, v, depth),
        _ => primitive_to_editable_py(py, v),
    }
}

/// Encodes a value with no declaring container context (a struct/pointer/embed field): a
/// primitive whose type is not reproduced by bare inference is wrapped in the explicit form.
fn value_to_editable_py<'py>(
    py: Python<'py>,
    v: &BinValue,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(parse_err(format!(
            "bin value nesting exceeds maximum depth of {MAX_DEPTH}"
        )));
    }
    Ok(match v {
        BinValue::List {
            is_list2,
            item,
            items,
        } => {
            let dict = PyDict::new(py);
            dict.set_item("__type__", if *is_list2 { "list2" } else { "list" })?;
            dict.set_item("item", type_name(*item))?;
            let list = PyList::empty(py);
            for it in items {
                list.append(value_to_editable_py_declared(py, it, depth + 1)?)?;
            }
            dict.set_item("items", list)?;
            dict.into_any()
        }
        BinValue::Map {
            key,
            value,
            entries,
        } => {
            let dict = PyDict::new(py);
            dict.set_item("__type__", "map")?;
            dict.set_item("key", type_name(*key))?;
            dict.set_item("value", type_name(*value))?;
            let list = PyList::empty(py);
            for (k, val) in entries {
                let k_py = value_to_editable_py_declared(py, k, depth + 1)?;
                let v_py = value_to_editable_py_declared(py, val, depth + 1)?;
                list.append((k_py, v_py))?;
            }
            dict.set_item("entries", list)?;
            dict.into_any()
        }
        BinValue::Pointer { class, fields } => {
            let dict = PyDict::new(py);
            dict.set_item("__type__", "pointer")?;
            dict.set_item("class", class)?;
            dict.set_item("fields", fields_to_editable_py(py, fields, depth + 1)?)?;
            dict.into_any()
        }
        BinValue::Embed { class, fields } => {
            let dict = PyDict::new(py);
            dict.set_item("__type__", "embed")?;
            dict.set_item("class", class)?;
            dict.set_item("fields", fields_to_editable_py(py, fields, depth + 1)?)?;
            dict.into_any()
        }
        BinValue::Option { item, value } => {
            let dict = PyDict::new(py);
            dict.set_item("__type__", "option")?;
            dict.set_item("item", type_name(*item))?;
            let inner = match value {
                Some(inner) => value_to_editable_py_declared(py, inner, depth + 1)?,
                None => py.None().into_bound(py),
            };
            dict.set_item("value", inner)?;
            dict.into_any()
        }
        _ => {
            if is_inferred(v.ty()) {
                primitive_to_editable_py(py, v)?
            } else {
                tagged_primitive(py, v)?
            }
        }
    })
}

fn fields_to_editable_py<'py>(
    py: Python<'py>,
    fields: &IndexMap<u32, BinValue>,
    depth: usize,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (name_hash, val) in fields {
        dict.set_item(name_hash, value_to_editable_py(py, val, depth)?)?;
    }
    Ok(dict)
}

fn doc_to_editable_py<'py>(py: Python<'py>, bin: &Bin) -> PyResult<Bound<'py, PyDict>> {
    let doc = PyDict::new(py);
    doc.set_item("version", bin.version)?;
    doc.set_item("is_patch", bin.is_patch)?;
    doc.set_item("patch_header", PyBytes::new(py, &bin.patch_header))?;
    doc.set_item("linked", bin.linked.clone())?;

    let entries = PyList::empty(py);
    for entry in &bin.entries {
        let e = PyDict::new(py);
        e.set_item("hash", entry.path_hash)?;
        e.set_item("class", entry.class_hash)?;
        e.set_item("fields", fields_to_editable_py(py, &entry.fields, 0)?)?;
        entries.append(e)?;
    }
    doc.set_item("entries", entries)?;

    let patches = PyList::empty(py);
    for patch in &bin.patches {
        let p = PyDict::new(py);
        p.set_item("key_hash", patch.key_hash)?;
        p.set_item("path", &patch.path)?;
        p.set_item("value", value_to_editable_py(py, &patch.value, 0)?)?;
        patches.append(p)?;
    }
    doc.set_item("patches", patches)?;
    Ok(doc)
}

#[pyfunction]
fn read_bin_editable(py: Python<'_>, path: std::path::PathBuf) -> PyResult<Bound<'_, PyDict>> {
    let bin = Bin::from_path(path).map_err(parse_err)?;
    doc_to_editable_py(py, &bin)
}

#[pyfunction]
fn read_bin_editable_bytes<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let bin = Bin::from_bytes(data).map_err(parse_err)?;
    doc_to_editable_py(py, &bin)
}

struct Ctx(String);

impl Ctx {
    fn root() -> Self {
        Self(String::new())
    }

    fn field(&self, key: u32) -> Self {
        Self(format!("{}.fields[{key:#x}]", self.0))
    }

    fn entry(&self, index: usize) -> Self {
        Self(format!("{}entries[{index}]", self.0))
    }

    fn patch(&self, index: usize) -> Self {
        Self(format!("{}patches[{index}]", self.0))
    }

    fn item(&self, index: usize) -> Self {
        Self(format!("{}.items[{index}]", self.0))
    }

    fn key(&self) -> Self {
        Self(format!("{}.key", self.0))
    }

    fn value(&self) -> Self {
        Self(format!("{}.value", self.0))
    }

    fn entry_at(&self, index: usize) -> Self {
        Self(format!("{}.entries[{index}]", self.0))
    }

    fn err(&self, msg: impl std::fmt::Display) -> PyErr {
        if self.0.is_empty() {
            write_err(msg)
        } else {
            write_err(format!("{}: {msg}", self.0))
        }
    }
}

fn extract_i128(any: &Bound<'_, PyAny>, ctx: &Ctx, ty: &str) -> PyResult<i128> {
    if any.is_instance_of::<PyBool>() {
        return Err(ctx.err(format!("expected int for value of type '{ty}', got bool")));
    }
    any.extract::<i128>().map_err(|_| {
        ctx.err(format!(
            "expected int for value of type '{ty}', got {}",
            type_of(any)
        ))
    })
}

fn type_of(any: &Bound<'_, PyAny>) -> String {
    any.get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "?".to_string())
}

fn range_err(ctx: &Ctx, ty: &str, n: i128) -> PyErr {
    ctx.err(format!("value {n} out of range for type '{ty}'"))
}

fn py_to_primitive(any: &Bound<'_, PyAny>, ty: BinType, ctx: &Ctx) -> PyResult<BinValue> {
    let name = type_name(ty);
    Ok(match ty {
        BinType::None => {
            if !any.is_none() {
                return Err(ctx.err(format!(
                    "expected None for type 'none', got {}",
                    type_of(any)
                )));
            }
            BinValue::None
        }
        BinType::Bool => {
            let b = any.downcast::<PyBool>().map_err(|_| {
                ctx.err(format!(
                    "expected bool for type 'bool', got {}",
                    type_of(any)
                ))
            })?;
            BinValue::Bool(b.is_true())
        }
        BinType::Flag => {
            let b = any.downcast::<PyBool>().map_err(|_| {
                ctx.err(format!(
                    "expected bool for type 'flag', got {}",
                    type_of(any)
                ))
            })?;
            BinValue::Flag(b.is_true())
        }
        BinType::I8 => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::I8(i8::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::U8 => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::U8(u8::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::I16 => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::I16(i16::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::U16 => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::U16(u16::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::I32 => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::I32(i32::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::U32 => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::U32(u32::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::I64 => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::I64(i64::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::U64 => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::U64(u64::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::Hash => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::Hash(u32::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::Link => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::Link(u32::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::File => {
            let n = extract_i128(any, ctx, name)?;
            BinValue::File(u64::try_from(n).map_err(|_| range_err(ctx, name, n))?)
        }
        BinType::F32 => {
            let f = any
                .downcast::<PyFloat>()
                .map(|f| f.value())
                .or_else(|_| any.extract::<i128>().map(|n| n as f64))
                .map_err(|_| {
                    ctx.err(format!(
                        "expected float for type 'f32', got {}",
                        type_of(any)
                    ))
                })?;
            BinValue::F32(f as f32)
        }
        BinType::String => {
            let s = any.downcast::<PyString>().map_err(|_| {
                ctx.err(format!(
                    "expected str for type 'string', got {}",
                    type_of(any)
                ))
            })?;
            BinValue::String(s.to_string())
        }
        BinType::Vec2 => BinValue::Vec2(extract_float_tuple::<2>(any, ctx, "vec2")?),
        BinType::Vec3 => BinValue::Vec3(extract_float_tuple::<3>(any, ctx, "vec3")?),
        BinType::Vec4 => BinValue::Vec4(extract_float_tuple::<4>(any, ctx, "vec4")?),
        BinType::Mtx44 => BinValue::Mtx44(extract_float_tuple::<16>(any, ctx, "mtx44")?),
        BinType::Rgba => {
            let tup = any.downcast::<PyTuple>().map_err(|_| {
                ctx.err(format!(
                    "expected 4-tuple for type 'rgba', got {}",
                    type_of(any)
                ))
            })?;
            if tup.len() != 4 {
                return Err(ctx.err(format!(
                    "expected 4-tuple for type 'rgba', got tuple of length {}",
                    tup.len()
                )));
            }
            let mut out = [0u8; 4];
            for (i, v) in tup.iter().enumerate() {
                let n = extract_i128(&v, ctx, "rgba")?;
                out[i] = u8::try_from(n).map_err(|_| range_err(ctx, "rgba", n))?;
            }
            BinValue::Rgba(out)
        }
        BinType::List
        | BinType::List2
        | BinType::Map
        | BinType::Pointer
        | BinType::Embed
        | BinType::Option => {
            return Err(ctx.err(format!(
                "type '{name}' is a container and cannot appear as a bare primitive value"
            )));
        }
    })
}

fn extract_float_tuple<const N: usize>(
    any: &Bound<'_, PyAny>,
    ctx: &Ctx,
    ty: &str,
) -> PyResult<[f32; N]> {
    let values: Vec<Bound<'_, PyAny>> = if let Ok(tup) = any.downcast::<PyTuple>() {
        tup.iter().collect()
    } else if let Ok(list) = any.downcast::<PyList>() {
        list.iter().collect()
    } else {
        return Err(ctx.err(format!(
            "expected {N}-tuple for type '{ty}', got {}",
            type_of(any)
        )));
    };
    if values.len() != N {
        return Err(ctx.err(format!(
            "expected {N}-tuple for type '{ty}', got sequence of length {}",
            values.len()
        )));
    }
    let mut out = [0f32; N];
    for (i, v) in values.iter().enumerate() {
        let f = v
            .downcast::<PyFloat>()
            .map(|f| f.value())
            .or_else(|_| v.extract::<i128>().map(|n| n as f64))
            .map_err(|_| {
                ctx.err(format!(
                    "expected number in '{ty}' tuple, got {}",
                    type_of(v)
                ))
            })?;
        out[i] = f as f32;
    }
    Ok(out)
}

fn infer_type(any: &Bound<'_, PyAny>) -> Option<BinType> {
    if any.is_none() {
        Some(BinType::None)
    } else if any.is_instance_of::<PyBool>() {
        Some(BinType::Bool)
    } else if any.is_instance_of::<PyFloat>() {
        Some(BinType::F32)
    } else if any.extract::<i128>().is_ok() {
        Some(BinType::I32)
    } else if any.is_instance_of::<PyString>() {
        Some(BinType::String)
    } else {
        None
    }
}

fn py_to_value(any: &Bound<'_, PyAny>, depth: usize, ctx: &Ctx) -> PyResult<BinValue> {
    if depth > MAX_DEPTH {
        return Err(ctx.err(format!(
            "bin value nesting exceeds maximum depth of {MAX_DEPTH}"
        )));
    }
    if let Ok(dict) = any.downcast::<PyDict>() {
        return dict_to_value(dict, depth, ctx);
    }
    let ty = infer_type(any).ok_or_else(|| {
        ctx.err(format!(
            "cannot infer a bin type for bare {}; use the explicit {{\"__type__\": ..., \"value\": ...}} form",
            type_of(any)
        ))
    })?;
    py_to_primitive(any, ty, ctx)
}

fn py_to_value_declared(
    any: &Bound<'_, PyAny>,
    declared: BinType,
    depth: usize,
    ctx: &Ctx,
) -> PyResult<BinValue> {
    if depth > MAX_DEPTH {
        return Err(ctx.err(format!(
            "bin value nesting exceeds maximum depth of {MAX_DEPTH}"
        )));
    }
    if declared.is_container() || matches!(declared, BinType::Pointer | BinType::Embed) {
        let dict = any.downcast::<PyDict>().map_err(|_| {
            ctx.err(format!(
                "expected a {{\"__type__\": ...}} dict for declared type '{}', got {}",
                type_name(declared),
                type_of(any)
            ))
        })?;
        let value = dict_to_value(dict, depth, ctx)?;
        if value.ty() != declared {
            return Err(ctx.err(format!(
                "expected declared type '{}', got '{}'",
                type_name(declared),
                type_name(value.ty())
            )));
        }
        return Ok(value);
    }
    if let Ok(dict) = any.downcast::<PyDict>() {
        if dict.contains("__type__")? {
            let value = dict_to_value(dict, depth, ctx)?;
            if value.ty() != declared {
                return Err(ctx.err(format!(
                    "expected declared type '{}', got '{}'",
                    type_name(declared),
                    type_name(value.ty())
                )));
            }
            return Ok(value);
        }
    }
    py_to_primitive(any, declared, ctx)
}

fn dict_to_value(dict: &Bound<'_, PyDict>, depth: usize, ctx: &Ctx) -> PyResult<BinValue> {
    let Some(ty_any) = dict.get_item("__type__")? else {
        return Err(ctx.err("expected a dict with a \"__type__\" key"));
    };
    let ty_str: String = ty_any
        .extract()
        .map_err(|_| ctx.err("\"__type__\" must be a string"))?;
    let ty =
        type_from_name(&ty_str).ok_or_else(|| ctx.err(format!("unknown __type__ '{ty_str}'")))?;

    match ty {
        BinType::List | BinType::List2 => {
            let item_any = dict
                .get_item("item")?
                .ok_or_else(|| ctx.err("list requires an \"item\" type declaration"))?;
            let item_str: String = item_any
                .extract()
                .map_err(|_| ctx.err("\"item\" must be a string"))?;
            let item = type_from_name(&item_str)
                .ok_or_else(|| ctx.err(format!("unknown item type '{item_str}'")))?;
            if item.is_container() {
                return Err(ctx.err(format!(
                    "list item type '{item_str}' is a container and cannot be a container element"
                )));
            }
            let items_any = dict
                .get_item("items")?
                .ok_or_else(|| ctx.err("list requires an \"items\" key"))?;
            let items_list = items_any.downcast::<PyList>().map_err(|_| {
                ctx.err(format!(
                    "\"items\" must be a list, got {}",
                    type_of(&items_any)
                ))
            })?;
            let mut items = Vec::with_capacity(items_list.len());
            for (i, elem) in items_list.iter().enumerate() {
                items.push(py_to_value_declared(&elem, item, depth + 1, &ctx.item(i))?);
            }
            Ok(BinValue::List {
                is_list2: matches!(ty, BinType::List2),
                item,
                items,
            })
        }
        BinType::Map => {
            let key_any = dict
                .get_item("key")?
                .ok_or_else(|| ctx.err("map requires a \"key\" type declaration"))?;
            let key_str: String = key_any
                .extract()
                .map_err(|_| ctx.err("\"key\" must be a string"))?;
            let key = type_from_name(&key_str)
                .ok_or_else(|| ctx.err(format!("unknown key type '{key_str}'")))?;
            if !key.is_primitive() {
                return Err(ctx.err(format!("map key type '{key_str}' must be a primitive")));
            }
            let value_any = dict
                .get_item("value")?
                .ok_or_else(|| ctx.err("map requires a \"value\" type declaration"))?;
            let value_str: String = value_any
                .extract()
                .map_err(|_| ctx.err("\"value\" must be a string"))?;
            let value = type_from_name(&value_str)
                .ok_or_else(|| ctx.err(format!("unknown value type '{value_str}'")))?;
            if value.is_container() {
                return Err(ctx.err(format!(
                    "map value type '{value_str}' is a container and cannot be a container element"
                )));
            }
            let entries_any = dict
                .get_item("entries")?
                .ok_or_else(|| ctx.err("map requires an \"entries\" key"))?;
            let entries_list = entries_any.downcast::<PyList>().map_err(|_| {
                ctx.err(format!(
                    "\"entries\" must be a list of 2-tuples, got {}",
                    type_of(&entries_any)
                ))
            })?;
            let mut entries = Vec::with_capacity(entries_list.len());
            for (i, elem) in entries_list.iter().enumerate() {
                let pair = elem.downcast::<PyTuple>().map_err(|_| {
                    ctx.entry_at(i)
                        .err(format!("expected a 2-tuple, got {}", type_of(&elem)))
                })?;
                if pair.len() != 2 {
                    return Err(ctx.entry_at(i).err(format!(
                        "expected a 2-tuple, got tuple of length {}",
                        pair.len()
                    )));
                }
                let k = pair.get_item(0)?;
                let v = pair.get_item(1)?;
                let k_val = py_to_value_declared(&k, key, depth + 1, &ctx.entry_at(i).key())?;
                let v_val = py_to_value_declared(&v, value, depth + 1, &ctx.entry_at(i).value())?;
                entries.push((k_val, v_val));
            }
            Ok(BinValue::Map {
                key,
                value,
                entries,
            })
        }
        BinType::Pointer | BinType::Embed => {
            let class_any = dict
                .get_item("class")?
                .ok_or_else(|| ctx.err(format!("{ty_str} requires a \"class\" key")))?;
            let class = extract_i128(&class_any, ctx, "class")?;
            let class = u32::try_from(class).map_err(|_| range_err(ctx, "class", class))?;
            let fields_any = dict
                .get_item("fields")?
                .ok_or_else(|| ctx.err(format!("{ty_str} requires a \"fields\" key")))?;
            let fields = py_to_fields(&fields_any, depth + 1, ctx)?;
            Ok(if matches!(ty, BinType::Pointer) {
                BinValue::Pointer { class, fields }
            } else {
                BinValue::Embed { class, fields }
            })
        }
        BinType::Option => {
            let item_any = dict
                .get_item("item")?
                .ok_or_else(|| ctx.err("option requires an \"item\" type declaration"))?;
            let item_str: String = item_any
                .extract()
                .map_err(|_| ctx.err("\"item\" must be a string"))?;
            let item = type_from_name(&item_str)
                .ok_or_else(|| ctx.err(format!("unknown item type '{item_str}'")))?;
            if item.is_container() {
                return Err(ctx.err(format!(
                    "option item type '{item_str}' is a container and cannot be a container element"
                )));
            }
            let value_any = dict
                .get_item("value")?
                .ok_or_else(|| ctx.err("option requires a \"value\" key"))?;
            let value = if value_any.is_none() {
                None
            } else {
                Some(Box::new(py_to_value_declared(
                    &value_any,
                    item,
                    depth + 1,
                    &ctx.value(),
                )?))
            };
            Ok(BinValue::Option { item, value })
        }
        other => {
            let value_any = dict.get_item("value")?.ok_or_else(|| {
                ctx.err(format!("explicit type '{ty_str}' requires a \"value\" key"))
            })?;
            py_to_primitive(&value_any, other, ctx)
        }
    }
}

fn py_to_fields(
    any: &Bound<'_, PyAny>,
    depth: usize,
    ctx: &Ctx,
) -> PyResult<IndexMap<u32, BinValue>> {
    let dict = any
        .downcast::<PyDict>()
        .map_err(|_| ctx.err(format!("\"fields\" must be a dict, got {}", type_of(any))))?;
    let mut out = IndexMap::with_capacity(dict.len());
    for (k, v) in dict.iter() {
        if k.is_instance_of::<PyBool>() {
            return Err(ctx.err("field key must be an int, not bool"));
        }
        let hash: i128 = k
            .extract()
            .map_err(|_| ctx.err(format!("field key must be an int, got {}", type_of(&k))))?;
        let hash = u32::try_from(hash)
            .map_err(|_| ctx.err(format!("field key {hash} out of range for u32")))?;
        let val = py_to_value(&v, depth, &ctx.field(hash))?;
        out.insert(hash, val);
    }
    Ok(out)
}

fn py_to_bin(py: Python<'_>, doc: &Bound<'_, PyDict>) -> PyResult<Bin> {
    let root = Ctx::root();
    let version: u32 = doc
        .get_item("version")?
        .ok_or_else(|| root.err("document requires a \"version\" key"))?
        .extract()
        .map_err(|_| root.err("\"version\" must be an int"))?;
    let is_patch: bool = doc
        .get_item("is_patch")?
        .ok_or_else(|| root.err("document requires an \"is_patch\" key"))?
        .extract()
        .map_err(|_| root.err("\"is_patch\" must be a bool"))?;
    let patch_header_any = doc
        .get_item("patch_header")?
        .ok_or_else(|| root.err("document requires a \"patch_header\" key"))?;
    let patch_header_bytes = patch_header_any
        .downcast::<PyBytes>()
        .map_err(|_| {
            root.err(format!(
                "\"patch_header\" must be bytes, got {}",
                type_of(&patch_header_any)
            ))
        })?
        .as_bytes();
    if patch_header_bytes.len() != 8 {
        return Err(root.err(format!(
            "\"patch_header\" must be exactly 8 bytes, got {}",
            patch_header_bytes.len()
        )));
    }
    let mut patch_header = [0u8; 8];
    patch_header.copy_from_slice(patch_header_bytes);

    let linked: Vec<String> = doc
        .get_item("linked")?
        .ok_or_else(|| root.err("document requires a \"linked\" key"))?
        .extract()
        .map_err(|_| root.err("\"linked\" must be a list of str"))?;

    let entries_any = doc
        .get_item("entries")?
        .ok_or_else(|| root.err("document requires an \"entries\" key"))?;
    let entries_list = entries_any.downcast::<PyList>().map_err(|_| {
        root.err(format!(
            "\"entries\" must be a list, got {}",
            type_of(&entries_any)
        ))
    })?;
    let mut entries = Vec::with_capacity(entries_list.len());
    for (i, elem) in entries_list.iter().enumerate() {
        let ectx = root.entry(i);
        let edict = elem
            .downcast::<PyDict>()
            .map_err(|_| ectx.err(format!("entry must be a dict, got {}", type_of(&elem))))?;
        let path_hash: i128 = edict
            .get_item("hash")?
            .ok_or_else(|| ectx.err("entry requires a \"hash\" key"))?
            .extract()
            .map_err(|_| ectx.err("entry \"hash\" must be an int"))?;
        let path_hash = u32::try_from(path_hash)
            .map_err(|_| ectx.err(format!("entry hash {path_hash} out of range for u32")))?;
        let class_hash: i128 = edict
            .get_item("class")?
            .ok_or_else(|| ectx.err("entry requires a \"class\" key"))?
            .extract()
            .map_err(|_| ectx.err("entry \"class\" must be an int"))?;
        let class_hash = u32::try_from(class_hash)
            .map_err(|_| ectx.err(format!("entry class {class_hash} out of range for u32")))?;
        let fields_any = edict
            .get_item("fields")?
            .ok_or_else(|| ectx.err("entry requires a \"fields\" key"))?;
        let fields = py_to_fields(&fields_any, 1, &ectx)?;
        entries.push(BinEntry {
            path_hash,
            class_hash,
            fields,
        });
    }

    let patches_any = doc
        .get_item("patches")?
        .ok_or_else(|| root.err("document requires a \"patches\" key"))?;
    let patches_list = patches_any.downcast::<PyList>().map_err(|_| {
        root.err(format!(
            "\"patches\" must be a list, got {}",
            type_of(&patches_any)
        ))
    })?;
    let mut patches = Vec::with_capacity(patches_list.len());
    for (i, elem) in patches_list.iter().enumerate() {
        let pctx = root.patch(i);
        let pdict = elem
            .downcast::<PyDict>()
            .map_err(|_| pctx.err(format!("patch must be a dict, got {}", type_of(&elem))))?;
        let key_hash: i128 = pdict
            .get_item("key_hash")?
            .ok_or_else(|| pctx.err("patch requires a \"key_hash\" key"))?
            .extract()
            .map_err(|_| pctx.err("patch \"key_hash\" must be an int"))?;
        let key_hash = u32::try_from(key_hash)
            .map_err(|_| pctx.err(format!("patch key_hash {key_hash} out of range for u32")))?;
        let path: String = pdict
            .get_item("path")?
            .ok_or_else(|| pctx.err("patch requires a \"path\" key"))?
            .extract()
            .map_err(|_| pctx.err("patch \"path\" must be a str"))?;
        let value_any = pdict
            .get_item("value")?
            .ok_or_else(|| pctx.err("patch requires a \"value\" key"))?;
        let value = py_to_value(&value_any, 1, &pctx.value())?;
        patches.push(BinPatch {
            key_hash,
            path,
            value,
        });
    }

    let _ = py;
    Ok(Bin {
        is_patch,
        patch_header,
        version,
        linked,
        entries,
        patches,
    })
}

#[pyfunction]
fn write_bin(py: Python<'_>, path: std::path::PathBuf, doc: &Bound<'_, PyDict>) -> PyResult<()> {
    let bin = py_to_bin(py, doc)?;
    bin.to_path(path).map_err(write_err)
}

#[pyfunction]
fn write_bin_bytes<'py>(
    py: Python<'py>,
    doc: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyBytes>> {
    let bin = py_to_bin(py, doc)?;
    let data = bin.to_bytes().map_err(write_err)?;
    Ok(PyBytes::new(py, &data))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(read_bin, m)?)?;
    m.add_function(wrap_pyfunction!(read_bin_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(read_bin_editable, m)?)?;
    m.add_function(wrap_pyfunction!(read_bin_editable_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(write_bin, m)?)?;
    m.add_function(wrap_pyfunction!(write_bin_bytes, m)?)?;
    Ok(())
}
