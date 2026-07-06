use indexmap::IndexMap;
use rs_hash::{HashMapper, fnv1a, xxh64};

use crate::bin::{Bin, BinEntry, BinPatch, BinType, BinValue};
use crate::error::{Error, Result};

/** Parses the `#PROP_text` form back into a [`Bin`]. The grammar mirrors ritobin's text reader: a
header line selecting `PROP`/`PTCH`, then `name: type = value` sections (`version`, `linked`,
`entries`, optional `patches`, and a tolerated `type`). Values are read recursively, with hashes
accepted either as `0xHEX` or as a bareword/quoted string that is hashed (FNV1a-32 for hash/link/
field/class names, XXH64 for file values). The `mapper` argument is accepted for symmetry with the
printer but is not consulted: names hash deterministically, so resolution is never required to
reconstruct the integer source of truth. */
pub fn from_text(text: &str, _mapper: Option<&HashMapper>) -> Result<Bin> {
    let mut p = Parser::new(text);
    p.parse_bin()
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            src: text.as_bytes(),
            pos: 0,
        }
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T> {
        Err(Error::TextParse {
            line: self.line(),
            message: message.into(),
        })
    }

    fn line(&self) -> usize {
        1 + self.src[..self.pos.min(self.src.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
    }

    fn eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_inline(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Skips inline whitespace, newlines, and `#` comments to the start of the next token, reporting
    /// whether at least one newline was crossed (a statement separator in the ritobin grammar).
    fn skip_newlines(&mut self) -> bool {
        let mut comment = false;
        let mut newline = false;
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\r' {
                self.pos += 1;
            } else if b == b'\n' {
                comment = false;
                newline = true;
                self.pos += 1;
            } else if b == b'#' {
                comment = true;
                self.pos += 1;
            } else if comment {
                self.pos += 1;
            } else {
                break;
            }
        }
        newline
    }

    fn read_symbol(&mut self, sym: u8) -> bool {
        self.skip_inline();
        if self.peek() == Some(sym) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, sym: u8) -> Result<()> {
        if self.read_symbol(sym) {
            Ok(())
        } else {
            self.err(format!("expected '{}'", sym as char))
        }
    }

    /// Reads a bareword of `[A-Za-z0-9_+-.]`, as used for type names, numbers, booleans, hex hashes,
    /// and unquoted identifiers.
    fn read_word(&mut self) -> &'a str {
        self.skip_inline();
        let start = self.pos;
        while let Some(b) = self.peek() {
            let ok = b == b'_' || b == b'+' || b == b'-' || b == b'.' || b.is_ascii_alphanumeric();
            if ok {
                self.pos += 1;
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("")
    }

    fn read_string(&mut self) -> Result<String> {
        self.skip_inline();
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return self.err("expected string literal"),
        };
        self.pos += 1;
        let mut out = String::new();
        // The source is a `&str`, so any run of bytes that is neither the closing
        // quote nor a backslash escape is already valid UTF-8 — copy the whole run
        // in one shot instead of decoding a char at a time. Only escapes take the
        // slow per-char path. (The old code called `str::from_utf8` on the *entire
        // remaining input* for every character, which was O(n) per char → O(n²)
        // over the file and made large VFX bins take tens of seconds to parse.)
        loop {
            let start = self.pos;
            while let Some(b) = self.peek() {
                if b == quote || b == b'\\' {
                    break;
                }
                self.pos += 1;
            }
            if self.pos > start {
                // SAFETY-equivalent: bytes came from a `&str`, so this run is valid
                // UTF-8. Use the checked conversion (still O(run), not O(rest)).
                match std::str::from_utf8(&self.src[start..self.pos]) {
                    Ok(s) => out.push_str(s),
                    Err(_) => return self.err("invalid utf-8 in string"),
                }
            }
            match self.peek() {
                None => return self.err("unterminated string literal"),
                Some(b) if b == quote => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.read_escape(&mut out)?;
                }
                // The inner loop only stops on quote / backslash / EOF, all handled
                // above, so this arm is unreachable.
                Some(_) => unreachable!(),
            }
        }
        Ok(out)
    }

    fn read_escape(&mut self, out: &mut String) -> Result<()> {
        let c = match self.peek() {
            Some(c) => c,
            None => return self.err("dangling escape"),
        };
        self.pos += 1;
        match c {
            b'"' => out.push('"'),
            b'\'' => out.push('\''),
            b'\\' => out.push('\\'),
            b'a' => out.push('\u{07}'),
            b'b' => out.push('\u{08}'),
            b'f' => out.push('\u{0C}'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'\n' => out.push('\n'),
            b'x' => {
                let v = self.read_hex_digits(2)?;
                out.push(char::from(v as u8));
            }
            b'u' => {
                let v = self.read_hex_digits(4)?;
                out.push(char::from_u32(v).unwrap_or('\u{FFFD}'));
            }
            other => return self.err(format!("unknown escape \\{}", other as char)),
        }
        Ok(())
    }

    fn read_hex_digits(&mut self, n: usize) -> Result<u32> {
        let mut v: u32 = 0;
        for _ in 0..n {
            let d = match self.peek().and_then(|b| (b as char).to_digit(16)) {
                Some(d) => d,
                None => return self.err("expected hex digit"),
            };
            self.pos += 1;
            v = v * 16 + d;
        }
        Ok(v)
    }

    fn read_typename(&mut self) -> Result<BinType> {
        let word = self.read_word();
        type_from_name(word)
            .ok_or(())
            .or_else(|_| self.err(format!("unknown type name '{word}'")))
    }

    /// Reads the `: type` annotation, returning the declared element/key/value tags for containers.
    fn read_value_type(&mut self) -> Result<TypeSpec> {
        self.expect_symbol(b':')?;
        let ty = self.read_typename()?;
        match ty {
            BinType::List | BinType::List2 | BinType::Option => {
                self.expect_symbol(b'[')?;
                let item = self.read_typename()?;
                if item.is_container() {
                    return self.err("container element may not be a container");
                }
                self.expect_symbol(b']')?;
                Ok(TypeSpec::Container {
                    outer: ty,
                    key: None,
                    item,
                })
            }
            BinType::Map => {
                self.expect_symbol(b'[')?;
                let key = self.read_typename()?;
                if !key.is_primitive() {
                    return self.err("map key must be primitive");
                }
                self.expect_symbol(b',')?;
                let item = self.read_typename()?;
                if item.is_container() {
                    return self.err("map value may not be a container");
                }
                self.expect_symbol(b']')?;
                Ok(TypeSpec::Container {
                    outer: ty,
                    key: Some(key),
                    item,
                })
            }
            other => Ok(TypeSpec::Simple(other)),
        }
    }

    fn parse_bin(&mut self) -> Result<Bin> {
        self.skip_newlines();
        let mut bin = Bin::new();
        let mut saw_type = false;
        if self.peek() == Some(b'#') {
            // header line such as `#PROP_text` / `#PTCH_text`
            let line_start = self.pos;
            while let Some(b) = self.peek() {
                if b == b'\n' {
                    break;
                }
                self.pos += 1;
            }
            let header = std::str::from_utf8(&self.src[line_start..self.pos]).unwrap_or("");
            let header = header.trim();
            if header.starts_with("#PTCH") {
                bin.is_patch = true;
                saw_type = true;
            } else if header.starts_with("#PROP") {
                saw_type = true;
            }
            self.skip_newlines();
        }

        let mut have_version = false;
        while !self.eof() {
            let name = self.read_word();
            if name.is_empty() {
                return self.err("expected section name");
            }
            let spec = self.read_value_type()?;
            self.expect_symbol(b'=')?;
            match name {
                "type" => {
                    let s = self.read_string()?;
                    bin.is_patch = s == "PTCH";
                    saw_type = true;
                }
                "version" => {
                    bin.version = self.read_u32_value(spec)?;
                    have_version = true;
                }
                "linked" => {
                    bin.linked = self.read_linked(spec)?;
                }
                "entries" => {
                    bin.entries = self.read_entries(spec)?;
                }
                "patches" => {
                    bin.patches = self.read_patches(spec)?;
                    bin.is_patch = true;
                }
                other => return self.err(format!("unknown section '{other}'")),
            }
            if !self.eof() && !self.read_separator() {
                return self.err("expected newline or ',' after section");
            }
        }

        if !have_version {
            return self.err("missing version section");
        }
        let _ = saw_type;
        if bin.is_patch {
            // The text form does not carry the raw `PTCH` header bytes; reconstruct the canonical
            // `version = 1, count = 0` header that override bins are written with.
            bin.patch_header = [1, 0, 0, 0, 0, 0, 0, 0];
        }
        Ok(bin)
    }

    fn read_u32_value(&mut self, spec: TypeSpec) -> Result<u32> {
        if spec != TypeSpec::Simple(BinType::U32) {
            return self.err("version must be u32");
        }
        let word = self.read_word();
        word.parse::<u32>()
            .map_err(|_| ())
            .or_else(|_| self.err(format!("invalid u32 '{word}'")))
    }

    fn read_linked(&mut self, spec: TypeSpec) -> Result<Vec<String>> {
        match spec {
            TypeSpec::Container {
                outer: BinType::List | BinType::List2,
                item: BinType::String,
                ..
            } => {}
            _ => return self.err("linked must be list[string]"),
        }
        let mut out = Vec::new();
        let mut end = self.read_nested_begin()?;
        while !end {
            out.push(self.read_string()?);
            end = self.read_separator_or_end()?;
        }
        Ok(out)
    }

    fn read_entries(&mut self, spec: TypeSpec) -> Result<Vec<BinEntry>> {
        match spec {
            TypeSpec::Container {
                outer: BinType::Map,
                key: Some(BinType::Hash),
                item: BinType::Embed,
            } => {}
            _ => return self.err("entries must be map[hash,embed]"),
        }
        let mut out = Vec::new();
        let mut end = self.read_nested_begin()?;
        while !end {
            let path_hash = self.read_hash32()?;
            self.expect_symbol(b'=')?;
            let class_hash = self.read_name_hash()?;
            let fields = self.read_field_block()?;
            out.push(BinEntry {
                path_hash,
                class_hash,
                fields,
            });
            end = self.read_separator_or_end()?;
        }
        Ok(out)
    }

    fn read_patches(&mut self, spec: TypeSpec) -> Result<Vec<BinPatch>> {
        match spec {
            TypeSpec::Container {
                outer: BinType::Map,
                key: Some(BinType::Hash),
                item: BinType::Embed,
            } => {}
            _ => return self.err("patches must be map[hash,embed]"),
        }
        let mut out = Vec::new();
        let mut end = self.read_nested_begin()?;
        while !end {
            let key_hash = self.read_hash32()?;
            self.expect_symbol(b'=')?;
            // embed name (e.g. `patch`) then a `{ path = ..., value = ... }` block
            let _ = self.read_name_hash()?;
            let fields = self.read_patch_block()?;
            out.push(BinPatch {
                key_hash,
                path: fields.0,
                value: fields.1,
            });
            end = self.read_separator_or_end()?;
        }
        Ok(out)
    }

    /// Reads the `{ path: string = "..."  value: T = v }` body of one patch embed.
    fn read_patch_block(&mut self) -> Result<(String, BinValue)> {
        let mut path: Option<String> = None;
        let mut value: Option<BinValue> = None;
        let mut end = self.read_nested_begin()?;
        while !end {
            let field = self.read_word();
            let spec = self.read_value_type()?;
            self.expect_symbol(b'=')?;
            let v = self.read_value(spec)?;
            match field {
                "path" => match v {
                    BinValue::String(s) => path = Some(s),
                    _ => return self.err("patch path must be a string"),
                },
                "value" => value = Some(v),
                other => return self.err(format!("unexpected patch field '{other}'")),
            }
            end = self.read_separator_or_end()?;
        }
        match (path, value) {
            (Some(p), Some(v)) => Ok((p, v)),
            _ => self.err("patch missing path or value"),
        }
    }

    fn read_field_block(&mut self) -> Result<IndexMap<u32, BinValue>> {
        let mut fields = IndexMap::new();
        let mut end = self.read_nested_begin()?;
        while !end {
            let name = self.read_name_hash()?;
            let spec = self.read_value_type()?;
            self.expect_symbol(b'=')?;
            let v = self.read_value(spec)?;
            fields.insert(name, v);
            end = self.read_separator_or_end()?;
        }
        Ok(fields)
    }

    fn read_value(&mut self, spec: TypeSpec) -> Result<BinValue> {
        match spec {
            TypeSpec::Simple(ty) => self.read_simple_value(ty),
            TypeSpec::Container { outer, key, item } => match outer {
                BinType::List | BinType::List2 => {
                    let mut items = Vec::new();
                    let mut end = self.read_nested_begin()?;
                    while !end {
                        items.push(self.read_simple_value(item)?);
                        end = self.read_separator_or_end()?;
                    }
                    Ok(BinValue::List {
                        is_list2: outer == BinType::List2,
                        item,
                        items,
                    })
                }
                BinType::Option => {
                    let mut value = None;
                    let mut end = self.read_nested_begin()?;
                    if !end {
                        value = Some(Box::new(self.read_simple_value(item)?));
                        end = self.read_separator_or_end()?;
                        if !end {
                            return self.err("option may hold at most one value");
                        }
                    }
                    Ok(BinValue::Option { item, value })
                }
                BinType::Map => {
                    let key = key.unwrap_or(BinType::Hash);
                    let mut entries = Vec::new();
                    let mut end = self.read_nested_begin()?;
                    while !end {
                        let k = self.read_simple_value(key)?;
                        self.expect_symbol(b'=')?;
                        let v = self.read_simple_value(item)?;
                        entries.push((k, v));
                        end = self.read_separator_or_end()?;
                    }
                    Ok(BinValue::Map {
                        key,
                        value: item,
                        entries,
                    })
                }
                _ => self.err("invalid container type"),
            },
        }
    }

    fn read_simple_value(&mut self, ty: BinType) -> Result<BinValue> {
        Ok(match ty {
            BinType::None => {
                let w = self.read_word();
                if w != "null" {
                    return self.err("expected null");
                }
                BinValue::None
            }
            BinType::Bool => BinValue::Bool(self.read_bool()?),
            BinType::Flag => BinValue::Flag(self.read_bool()?),
            BinType::I8 => BinValue::I8(self.read_number()?),
            BinType::U8 => BinValue::U8(self.read_number()?),
            BinType::I16 => BinValue::I16(self.read_number()?),
            BinType::U16 => BinValue::U16(self.read_number()?),
            BinType::I32 => BinValue::I32(self.read_number()?),
            BinType::U32 => BinValue::U32(self.read_number()?),
            BinType::I64 => BinValue::I64(self.read_number()?),
            BinType::U64 => BinValue::U64(self.read_number()?),
            BinType::F32 => BinValue::F32(self.read_number()?),
            BinType::Vec2 => BinValue::Vec2(self.read_float_array::<2>()?),
            BinType::Vec3 => BinValue::Vec3(self.read_float_array::<3>()?),
            BinType::Vec4 => BinValue::Vec4(self.read_float_array::<4>()?),
            BinType::Mtx44 => BinValue::Mtx44(self.read_mtx44()?),
            BinType::Rgba => {
                let a = self.read_u8_array::<4>()?;
                BinValue::Rgba(a)
            }
            BinType::String => BinValue::String(self.read_string()?),
            BinType::Hash => BinValue::Hash(self.read_hash32()?),
            BinType::Link => BinValue::Link(self.read_hash32()?),
            BinType::File => BinValue::File(self.read_hash64()?),
            BinType::Pointer => self.read_struct(false)?,
            BinType::Embed => self.read_struct(true)?,
            BinType::List | BinType::List2 | BinType::Map | BinType::Option => {
                return self.err("container type encountered as a scalar element");
            }
        })
    }

    fn read_struct(&mut self, is_embed: bool) -> Result<BinValue> {
        // either `null` (pointer) or `ClassName { fields }`
        let backup = self.pos;
        let word = self.read_word();
        if !is_embed && word == "null" {
            return Ok(BinValue::Pointer {
                class: 0,
                fields: IndexMap::new(),
            });
        }
        self.pos = backup;
        let class = self.read_name_hash()?;
        let fields = self.read_field_block()?;
        if is_embed {
            Ok(BinValue::Embed { class, fields })
        } else {
            Ok(BinValue::Pointer { class, fields })
        }
    }

    fn read_bool(&mut self) -> Result<bool> {
        match self.read_word() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => self.err(format!("expected bool, got '{other}'")),
        }
    }

    fn read_number<T: ParseNum>(&mut self) -> Result<T> {
        let word = self.read_word();
        T::parse_num(word)
            .map_err(|_| ())
            .or_else(|_| self.err(format!("invalid number '{word}'")))
    }

    fn read_float_array<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut out = [0.0f32; N];
        let mut i = 0;
        let mut end = self.read_nested_begin()?;
        while !end {
            if i >= N {
                return self.err("too many array elements");
            }
            out[i] = self.read_number::<f32>()?;
            i += 1;
            end = self.read_separator_or_end()?;
        }
        if i != N {
            return self.err("too few array elements");
        }
        Ok(out)
    }

    /// Reads a 4x4 matrix as 16 floats. The printer emits an `mtx44` as four
    /// nested brace rows:
    ///
    /// ```text
    /// {
    ///     { m00, m01, m02, m03 }
    ///     { m10, m11, m12, m13 }
    ///     { m20, m21, m22, m23 }
    ///     { m30, m31, m32, m33 }
    /// }
    /// ```
    ///
    /// so the generic flat `read_float_array::<16>` (which hits the first inner
    /// `{` where it expects a number) can't parse our own output. This reader is
    /// brace-tolerant: it tracks brace depth, reading 16 floats and treating any
    /// `{`/`}`/`,`/newlines as structure to skip until the outer brace closes.
    /// A flat `{ f, f, ... }` matrix parses too.
    fn read_mtx44(&mut self) -> Result<[f32; 16]> {
        let mut out = [0.0f32; 16];
        if !self.read_symbol(b'{') {
            return self.err("expected '{'");
        }
        // Outer brace consumed → depth 1. Read until it closes (depth 0).
        let mut depth = 1usize;
        let mut i = 0usize;
        while depth > 0 {
            self.skip_newlines();
            match self.peek() {
                Some(b'{') => {
                    self.pos += 1;
                    depth += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    depth -= 1;
                }
                Some(b',') => {
                    self.pos += 1;
                }
                Some(_) => {
                    if i >= 16 {
                        return self.err("too many matrix elements");
                    }
                    out[i] = self.read_number::<f32>()?;
                    i += 1;
                }
                None => return self.err("unterminated mtx44"),
            }
        }
        if i != 16 {
            return self.err("mtx44 needs 16 elements");
        }
        Ok(out)
    }

    fn read_u8_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        let mut i = 0;
        let mut end = self.read_nested_begin()?;
        while !end {
            if i >= N {
                return self.err("too many array elements");
            }
            out[i] = self.read_number::<u8>()?;
            i += 1;
            end = self.read_separator_or_end()?;
        }
        if i != N {
            return self.err("too few array elements");
        }
        Ok(out)
    }

    /// Reads a 32-bit hash: `0xHEX`, or a bareword/quoted string hashed with FNV1a-32.
    fn read_hash32(&mut self) -> Result<u32> {
        let backup = self.pos;
        if let Some(h) = self.try_read_hex32()? {
            return Ok(h);
        }
        self.pos = backup;
        self.skip_inline();
        if matches!(self.peek(), Some(b'"' | b'\'')) {
            let s = self.read_string()?;
            return Ok(fnv1a(&s));
        }
        let w = self.read_word();
        if w.is_empty() {
            return self.err("expected hash");
        }
        Ok(fnv1a(w))
    }

    /// Reads a class/field name: `0xHEX`, a bareword, or a quoted string, hashed with FNV1a-32. The
    /// printer renders resolved field and class names as barewords and entry keys as quoted strings,
    /// so all three spellings must round-trip to the same integer.
    fn read_name_hash(&mut self) -> Result<u32> {
        let backup = self.pos;
        if let Some(h) = self.try_read_hex32()? {
            return Ok(h);
        }
        self.pos = backup;
        self.skip_inline();
        if matches!(self.peek(), Some(b'"' | b'\'')) {
            let s = self.read_string()?;
            return Ok(fnv1a(&s));
        }
        let w = self.read_word();
        if w.is_empty() {
            return self.err("expected name");
        }
        Ok(fnv1a(w))
    }

    fn read_hash64(&mut self) -> Result<u64> {
        let backup = self.pos;
        if let Some(h) = self.try_read_hex64()? {
            return Ok(h);
        }
        self.pos = backup;
        self.skip_inline();
        if matches!(self.peek(), Some(b'"' | b'\'')) {
            let s = self.read_string()?;
            return Ok(xxh64(&s));
        }
        let w = self.read_word();
        if w.is_empty() {
            return self.err("expected file hash");
        }
        Ok(xxh64(w))
    }

    fn try_read_hex32(&mut self) -> Result<Option<u32>> {
        let word = self.read_word();
        if word.len() >= 2 && &word[..2].to_ascii_lowercase() == "0x" {
            match u32::from_str_radix(&word[2..], 16) {
                Ok(v) => Ok(Some(v)),
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    fn try_read_hex64(&mut self) -> Result<Option<u64>> {
        let word = self.read_word();
        if word.len() >= 2 && &word[..2].to_ascii_lowercase() == "0x" {
            match u64::from_str_radix(&word[2..], 16) {
                Ok(v) => Ok(Some(v)),
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    fn read_nested_begin(&mut self) -> Result<bool> {
        if !self.read_symbol(b'{') {
            return self.err("expected '{'");
        }
        self.skip_newlines();
        Ok(self.read_symbol(b'}'))
    }

    fn read_separator(&mut self) -> bool {
        if self.skip_newlines() {
            return true;
        }
        if self.read_symbol(b',') {
            self.skip_newlines();
            return true;
        }
        false
    }

    fn read_separator_or_end(&mut self) -> Result<bool> {
        if self.read_symbol(b'}') {
            return Ok(true);
        }
        if self.read_separator() {
            return Ok(self.read_symbol(b'}'));
        }
        self.err("expected separator or '}'")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeSpec {
    Simple(BinType),
    Container {
        outer: BinType,
        key: Option<BinType>,
        item: BinType,
    },
}

trait ParseNum: Sized {
    fn parse_num(s: &str) -> core::result::Result<Self, ()>;
}

macro_rules! impl_parse_int {
    ($($t:ty),*) => {$(
        impl ParseNum for $t {
            fn parse_num(s: &str) -> core::result::Result<Self, ()> {
                let s = s.strip_prefix('+').unwrap_or(s);
                s.parse::<$t>().map_err(|_| ())
            }
        }
    )*};
}

impl_parse_int!(i8, u8, i16, u16, i32, u32, i64, u64);

impl ParseNum for f32 {
    fn parse_num(s: &str) -> core::result::Result<Self, ()> {
        let s = s.strip_prefix('+').unwrap_or(s);
        s.parse::<f32>().map_err(|_| ())
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
