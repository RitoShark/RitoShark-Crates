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

/** Parses a SINGLE value expression, as produced by [`crate::text::value_to_text`], back into a
[`BinValue`]. This is the per-node inverse used by an editor that edits one subtree as raw text.

The grammar is the SAME grammar `from_text` uses: the whole body is read by the same `read_value` /
`read_struct` / `read_field_block` routines, so anything `value_to_text` emits parses here, and
`value_from_text(value_to_text(v)) == v` for every value whose root type survives the trip (see
below). An optional leading `type = ` annotation is accepted, so `pointer = Foo { ... }` and
`list[f32] = { 0 1 }` both work.

ROOT TYPE INFERENCE. A standalone value has no owning `name: type = ` line, so with no annotation
the root type is inferred from the first token:

- `Name { ... }` / `0xHEX { ... }` -> `pointer`. A bare class header is ambiguous between pointer and
  embed (both print identically), and POINTER is chosen because that is what emitter and other
  object lists hold. A caller that knows better should use [`value_from_text_as`], which is the
  path an editor replacing a typed node must take so an `embed` node does not silently become a
  `pointer` and fail the receiving type check.
- `"..."` -> `string`, `true`/`false` -> `bool`, `null` -> a null `pointer`, a numeric literal ->
  `f32` if it has a `.`/exponent else `i32` (or `u32`/`i64`/`u64` as magnitude requires).
- `{ ... }` -> rejected. A brace-only root could be a list, a map, an option, a vec, an rgba or an
  mtx44 with no way to tell them apart, and guessing would silently corrupt the node. Annotate it
  (`list[f32] = { ... }`) or call [`value_from_text_as`].

`mapper` is accepted for symmetry with the printer but not consulted, for the same reason
`from_text` ignores it: names hash deterministically, so the integer source of truth is
reconstructed without resolution. */
pub fn value_from_text(text: &str, _mapper: Option<&HashMapper>) -> Result<BinValue> {
    let mut p = Parser::new(text);
    p.parse_standalone_value(None)
}

/** [`value_from_text`] with the root type known up front, e.g. from the node being replaced.

`expected` is used only when the text carries no `type = ` annotation of its own; an explicit
annotation always wins, and a mismatch between the two is an error rather than a silent
reinterpretation. Pass the exact tag of the node being overwritten (`BinType::Pointer` for an
emitter in a `list[pointer]`, `BinType::Embed` for one in a `list[embed]`, and so on) so a
pointer/embed pair, which print identically, cannot swap places on the way back in.

For a container root (`list`, `list2`, `map`, `option`) the element tags cannot be recovered from an
unannotated body either, so `expected` alone is not enough: annotate the text (which is what
`value_to_text` output nested inside a field always carries) or replace the container from the
widget path instead. */
pub fn value_from_text_as(
    text: &str,
    expected: BinType,
    _mapper: Option<&HashMapper>,
) -> Result<BinValue> {
    let mut p = Parser::new(text);
    p.parse_standalone_value(Some(expected))
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

    /// Reads one standalone value expression and requires the input to be fully consumed.
    ///
    /// Trailing junk is an ERROR, not ignored: this backs a per-node text editor, and quietly
    /// dropping whatever follows the value would let a half-finished paste apply as a smaller,
    /// valid-looking node. `expected` is the caller's root type when the text has no annotation of
    /// its own (see [`super::value_from_text_as`]).
    fn parse_standalone_value(&mut self, expected: Option<BinType>) -> Result<BinValue> {
        self.skip_newlines();
        if self.eof() {
            return self.err("expected a value, got empty text");
        }
        let spec = self.read_standalone_spec(expected)?;
        let value = self.read_value(spec)?;
        self.skip_newlines();
        if !self.eof() {
            return self.err("unexpected trailing text after the value");
        }
        Ok(value)
    }

    /// Works out the root's [`TypeSpec`]: an explicit leading `type = ` annotation if present,
    /// else the caller's `expected`, else inference from the first token.
    fn read_standalone_spec(&mut self, expected: Option<BinType>) -> Result<TypeSpec> {
        // An annotation is `type = ...` / `list[f32] = ...`. Probe for it by speculatively reading a
        // type name and requiring the `=` (or a `[` for a container) to follow. Anything else means
        // the text starts with the value itself, so rewind and infer.
        let backup = self.pos;
        if let Some(spec) = self.try_read_annotation()? {
            if let (Some(expected), TypeSpec::Simple(got) | TypeSpec::Container { outer: got, .. }) =
                (expected, spec)
            {
                // A declared tag that contradicts the node being replaced is a real conflict: the
                // caller would reject the value anyway, and saying so here names the actual problem
                // instead of surfacing a downstream "type mismatch".
                if got != expected {
                    return self.err(format!(
                        "declared type '{}' does not match the expected type '{}'",
                        type_display(got),
                        type_display(expected)
                    ));
                }
            }
            return Ok(spec);
        }
        self.pos = backup;

        if let Some(ty) = expected {
            if ty.is_container() {
                return self.err(format!(
                    "a '{}' value needs its element types spelled out, e.g. '{} = {{ ... }}'",
                    type_display(ty),
                    type_display(ty)
                ));
            }
            return Ok(TypeSpec::Simple(ty));
        }
        self.infer_root_type().map(TypeSpec::Simple)
    }

    /// Reads a leading `type = ` / `list[item] = ` annotation, or `None` (position unspecified) when
    /// the text does not start with one.
    fn try_read_annotation(&mut self) -> Result<Option<TypeSpec>> {
        let word = self.read_word();
        let Some(ty) = type_from_name(word) else {
            return Ok(None);
        };
        if ty.is_container() {
            // `list`/`map`/`option` must be followed by their bracketed element tags. Reuse the same
            // reader the field path uses by rewinding to the type name and letting `read_value_type`
            // consume `: type[...]`; it wants a leading ':' which a standalone annotation lacks, so
            // the bracket part is read directly here instead.
            let spec = self.read_container_tags(ty)?;
            if !self.read_symbol(b'=') {
                return self.err("expected '=' after the container type annotation");
            }
            return Ok(Some(spec));
        }
        if !self.read_symbol(b'=') {
            // Not an annotation: a bareword that happens to name a type (a class literally called
            // `hash`, say) followed by its body. Let the caller rewind and infer.
            return Ok(None);
        }
        Ok(Some(TypeSpec::Simple(ty)))
    }

    /// Reads the `[item]` / `[key,value]` element tags of a container type. Shares every validation
    /// rule with `read_value_type` so the two grammars cannot drift apart.
    fn read_container_tags(&mut self, ty: BinType) -> Result<TypeSpec> {
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

    /// Guesses the root type of an unannotated standalone value from its first token. Never consumes
    /// input: it only peeks, so the real reader still sees the whole value.
    fn infer_root_type(&mut self) -> Result<BinType> {
        self.skip_newlines();
        match self.peek() {
            Some(b'"' | b'\'') => Ok(BinType::String),
            // Brace-only roots are genuinely undecidable (list / map / option / vec2..4 / rgba /
            // mtx44 all open with '{'), and guessing wrong would rewrite the node's type. Refuse.
            Some(b'{') => self.err(
                "cannot tell what a '{ ... }' value is; prefix it with its type, e.g. 'list[f32] = { ... }'",
            ),
            Some(_) => {
                let backup = self.pos;
                let word = self.read_word();
                self.pos = backup;
                if word.is_empty() {
                    return self.err("expected a value");
                }
                match word {
                    "true" | "false" => return Ok(BinType::Bool),
                    // A lone `null` is only ever a null pointer in the text form.
                    "null" => return Ok(BinType::Pointer),
                    _ => {}
                }
                if let Some(ty) = infer_number_type(word) {
                    return Ok(ty);
                }
                // A bareword or `0xHEX` followed by a body is a struct header. Pointer over embed:
                // they print identically and pointer is the shape object lists (emitters included)
                // hold. Callers that know the node's real tag use `value_from_text_as`.
                Ok(BinType::Pointer)
            }
            None => self.err("expected a value"),
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

    /// Reads a 4x4 matrix as 16 floats. Accepts both the canonical flat form
    /// (one brace, 16 bare floats) and the legacy per-row-brace form that a
    /// broken writer once emitted:
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
    /// The per-row form is not valid ritobin and only ever existed in files
    /// produced by that buggy writer; the generic flat `read_float_array::<16>`
    /// (which hits the first inner `{` where it expects a number) can't parse
    /// it. This reader is brace-tolerant: it tracks brace depth, reading 16
    /// floats and treating any `{`/`}`/`,`/newlines as structure to skip until
    /// the outer brace closes. The writer always emits the flat form, so
    /// re-saving a tolerated file silently repairs it.
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

/// Picks the narrowest sensible numeric type for an unannotated standalone number literal, or `None`
/// when the word is not a number at all.
///
/// Only reachable from [`Parser::infer_root_type`], i.e. when a caller supplied no expected type.
/// The rule is: anything with a `.` or an exponent is `f32`; an integer is `i32`, widening to `u32`
/// then `i64` then `u64` as its magnitude requires. Sub-32-bit tags (`u8`, `i16`, ...) are never
/// guessed because a bare `3` gives no signal at all, so a caller replacing a `u8` node must pass
/// the expected type (or annotate the text) rather than rely on inference.
fn infer_number_type(word: &str) -> Option<BinType> {
    let body = word.strip_prefix('+').or_else(|| word.strip_prefix('-')).unwrap_or(word);
    if body.is_empty() || !body.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    if word.contains('.') || word.contains('e') || word.contains('E') {
        return word.parse::<f32>().ok().map(|_| BinType::F32);
    }
    if word.parse::<i32>().is_ok() {
        return Some(BinType::I32);
    }
    if word.parse::<u32>().is_ok() {
        return Some(BinType::U32);
    }
    if word.parse::<i64>().is_ok() {
        return Some(BinType::I64);
    }
    if word.parse::<u64>().is_ok() {
        return Some(BinType::U64);
    }
    // Digits that fit no integer width: treat as a float rather than failing outright.
    word.parse::<f32>().ok().map(|_| BinType::F32)
}

/// The text-form spelling of a type tag, for error messages. Mirrors `print::type_name`, kept here
/// so the parser does not depend on the printer module.
fn type_display(ty: BinType) -> &'static str {
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
