use std::fmt::Write as _;

use indexmap::IndexMap;
use rs_hash::HashMapper;

use crate::bin::{Bin, BinType, BinValue};

/// Renders `bin` as `#PROP_text`. Field, class, entry, hash, and link names are resolved through
/// `mapper` when present; unresolved hashes fall back to `0x%08x` (or `0x%016x` for file hashes).
pub fn to_text(bin: &Bin, mapper: Option<&HashMapper>) -> String {
    let mut out = String::new();
    let header = if bin.is_patch {
        "#PTCH_text"
    } else {
        "#PROP_text"
    };
    let _ = writeln!(out, "{header}");
    let _ = writeln!(out, "version: u32 = {}", bin.version);

    if !bin.linked.is_empty() {
        let _ = writeln!(out, "linked: list[string] = {{");
        for path in &bin.linked {
            out.push_str("    ");
            push_string(&mut out, path);
            out.push('\n');
        }
        out.push_str("}\n");
    }

    let _ = writeln!(out, "entries: map[hash,embed] = {{");
    for entry in &bin.entries {
        out.push_str("    ");
        push_hash32(&mut out, entry.path_hash, mapper);
        out.push_str(" = ");
        push_name(&mut out, entry.class_hash, mapper);
        out.push_str(" {\n");
        push_fields(&mut out, &entry.fields, 2, mapper);
        out.push_str("    }\n");
    }
    out.push_str("}\n");

    if bin.is_patch {
        let _ = writeln!(out, "patches: map[hash,embed] = {{");
        for patch in &bin.patches {
            out.push_str("    ");
            push_hash32(&mut out, patch.key_hash, mapper);
            out.push_str(" = patch {\n");
            indent(&mut out, 2);
            out.push_str("path: string = ");
            push_string(&mut out, &patch.path);
            out.push('\n');
            indent(&mut out, 2);
            out.push_str("value: ");
            push_type(&mut out, &patch.value);
            out.push_str(" = ");
            push_value(&mut out, &patch.value, 2, mapper);
            out.push('\n');
            out.push_str("    }\n");
        }
        out.push_str("}\n");
    }

    out
}

fn push_fields(
    out: &mut String,
    fields: &IndexMap<u32, BinValue>,
    depth: usize,
    mapper: Option<&HashMapper>,
) {
    for (name, value) in fields {
        indent(out, depth);
        push_name(out, *name, mapper);
        out.push_str(": ");
        push_type(out, value);
        out.push_str(" = ");
        push_value(out, value, depth, mapper);
        out.push('\n');
    }
}

/// The full type label printed before a value, including the bracketed element types of
/// containers, e.g. `list[u32]`, `map[hash,embed]`, `option[string]`.
fn push_type(out: &mut String, value: &BinValue) {
    match value {
        BinValue::List { is_list2, item, .. } => {
            out.push_str(if *is_list2 { "list2" } else { "list" });
            out.push('[');
            out.push_str(type_name(*item));
            out.push(']');
        }
        BinValue::Map { key, value, .. } => {
            out.push_str("map[");
            out.push_str(type_name(*key));
            out.push(',');
            out.push_str(type_name(*value));
            out.push(']');
        }
        BinValue::Option { item, .. } => {
            out.push_str("option[");
            out.push_str(type_name(*item));
            out.push(']');
        }
        other => out.push_str(type_name(other.ty())),
    }
}

fn push_value(out: &mut String, value: &BinValue, depth: usize, mapper: Option<&HashMapper>) {
    match value {
        BinValue::None => out.push_str("null"),
        BinValue::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        BinValue::Flag(v) => out.push_str(if *v { "true" } else { "false" }),
        BinValue::I8(v) => {
            let _ = write!(out, "{v}");
        }
        BinValue::U8(v) => {
            let _ = write!(out, "{v}");
        }
        BinValue::I16(v) => {
            let _ = write!(out, "{v}");
        }
        BinValue::U16(v) => {
            let _ = write!(out, "{v}");
        }
        BinValue::I32(v) => {
            let _ = write!(out, "{v}");
        }
        BinValue::U32(v) => {
            let _ = write!(out, "{v}");
        }
        BinValue::I64(v) => {
            let _ = write!(out, "{v}");
        }
        BinValue::U64(v) => {
            let _ = write!(out, "{v}");
        }
        BinValue::F32(v) => push_float(out, *v),
        BinValue::Vec2(a) => push_floats(out, a),
        BinValue::Vec3(a) => push_floats(out, a),
        BinValue::Vec4(a) => push_floats(out, a),
        BinValue::Mtx44(a) => {
            // ritobin renders mtx44 as one brace holding 16 bare floats, four per line, with no
            // per-row braces. That flat form is the only shape ritobin's (and our) text reader
            // accepts, so the rows must NOT be wrapped like vec2/3/4. See parse::read_float_array.
            out.push_str("{\n");
            for row in 0..4 {
                indent(out, depth + 1);
                for col in 0..4 {
                    if col > 0 {
                        out.push_str(", ");
                    }
                    push_float(out, a[row * 4 + col]);
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push('}');
        }
        BinValue::Rgba(a) => {
            let _ = write!(out, "{{ {}, {}, {}, {} }}", a[0], a[1], a[2], a[3]);
        }
        BinValue::String(s) => push_string(out, s),
        BinValue::Hash(v) => push_hash32(out, *v, mapper),
        BinValue::Link(v) => push_hash32(out, *v, mapper),
        BinValue::File(v) => push_hash64(out, *v, mapper),
        BinValue::List { items, .. } => {
            if items.is_empty() {
                out.push_str("{}");
            } else {
                out.push_str("{\n");
                for v in items {
                    indent(out, depth + 1);
                    push_value(out, v, depth + 1, mapper);
                    out.push('\n');
                }
                indent(out, depth);
                out.push('}');
            }
        }
        BinValue::Map { entries, .. } => {
            if entries.is_empty() {
                out.push_str("{}");
            } else {
                out.push_str("{\n");
                for (k, v) in entries {
                    indent(out, depth + 1);
                    push_value(out, k, depth + 1, mapper);
                    out.push_str(" = ");
                    push_value(out, v, depth + 1, mapper);
                    out.push('\n');
                }
                indent(out, depth);
                out.push('}');
            }
        }
        BinValue::Option { value, .. } => match value {
            None => out.push_str("{}"),
            Some(v) => {
                out.push_str("{\n");
                indent(out, depth + 1);
                push_value(out, v, depth + 1, mapper);
                out.push('\n');
                indent(out, depth);
                out.push('}');
            }
        },
        BinValue::Pointer { class, fields } => {
            if *class == 0 {
                out.push_str("null");
                return;
            }
            push_struct(out, *class, fields, depth, mapper);
        }
        BinValue::Embed { class, fields } => push_struct(out, *class, fields, depth, mapper),
    }
}

fn push_struct(
    out: &mut String,
    class: u32,
    fields: &IndexMap<u32, BinValue>,
    depth: usize,
    mapper: Option<&HashMapper>,
) {
    push_name(out, class, mapper);
    if fields.is_empty() {
        out.push_str(" {}");
    } else {
        out.push_str(" {\n");
        push_fields(out, fields, depth + 1, mapper);
        indent(out, depth);
        out.push('}');
    }
}

fn push_floats(out: &mut String, vals: &[f32]) {
    out.push_str("{ ");
    for (i, v) in vals.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_float(out, *v);
    }
    out.push_str(" }");
}

fn push_float(out: &mut String, v: f32) {
    if v == v.trunc() && v.is_finite() && v.abs() < 1e16 {
        let _ = write!(out, "{v:.0}");
    } else {
        let _ = write!(out, "{v}");
    }
}

fn push_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

/** Renders a resolved field or class name. Canonical ritobin writes these as barewords (e.g.
`rate:`, `TestClass {`), unlike entry keys and hash/link values which are always quoted. A resolved
name that is not a valid identifier is quoted so the output stays parseable; an unresolved hash
falls back to `0x%08x`. */
fn push_name(out: &mut String, hash: u32, mapper: Option<&HashMapper>) {
    match mapper.and_then(|m| m.get(hash as u64)) {
        Some(name) if is_bareword(name) => out.push_str(name),
        Some(name) => push_string(out, name),
        None => {
            let _ = write!(out, "0x{hash:08x}");
        }
    }
}

/// Whether `s` is a valid ritobin identifier: `^[A-Za-z_][A-Za-z0-9_]*$`, so it can be written bare.
fn is_bareword(s: &str) -> bool {
    let mut chars = s.bytes();
    match chars.next() {
        Some(b) if b == b'_' || b.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|b| b == b'_' || b.is_ascii_alphanumeric())
}

fn push_hash32(out: &mut String, hash: u32, mapper: Option<&HashMapper>) {
    match mapper.and_then(|m| m.get(hash as u64)) {
        Some(name) => push_string(out, name),
        None => {
            let _ = write!(out, "0x{hash:08x}");
        }
    }
}

fn push_hash64(out: &mut String, hash: u64, mapper: Option<&HashMapper>) {
    match mapper.and_then(|m| m.get(hash)) {
        Some(name) => push_string(out, name),
        None => {
            let _ = write!(out, "0x{hash:016x}");
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
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
