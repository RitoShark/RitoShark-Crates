/*!
The `ritobinmap` record: the paths a mod invented, kept as a normal bin entry.

Riot is migrating asset references from readable strings to hashes, and a path a mod
invented is in no dictionary anywhere, so the pair has to be captured at the one instant
a tool knows both. The record is a plain top-level entry — class `RitoBinMap`, one
`list[string]` per hash category — so it is ordinary bin data: every parser reads it,
rewrites it and carries it through a `bin -> text -> bin` round-trip, and nothing has to
know the record exists to avoid choking on it.

```text
ritobinmap = RitoBinMap {
    binEntries: list[string]  entry paths and `link` values   (FNV1a-32)
    binTypes:   list[string]  class names                     (FNV1a-32)
    binFields:  list[string]  field names                     (FNV1a-32)
    binHashes:  list[string]  `hash` values                   (FNV1a-32)
    game:       list[string]  `file` values                   (XXH64)
}
```

Only paths are stored: the hash is `fnv1a`/`xxh64` of the path, so writing it too would be
redundant bytes that compress badly. The split by category is what makes a lookup safe — a
`file` hash is only ever named from `game`, never from an unrelated name that collided.
*/

use std::collections::{BTreeMap, BTreeSet};

use indexmap::IndexMap;
use rs_hash::{fnv1a, xxh64};

use crate::bin::{Bin, BinEntry, BinType, BinValue};

pub const RITOBINMAP_ENTRY_NAME: &str = "ritobinmap";
pub const RITOBINMAP_CLASS_NAME: &str = "RitoBinMap";
pub const RITOBINMAP_ENTRY: u32 = fnv1a(RITOBINMAP_ENTRY_NAME);
pub const RITOBINMAP_CLASS: u32 = fnv1a(RITOBINMAP_CLASS_NAME);

const BIN_ENTRIES: u32 = fnv1a("binEntries");
const BIN_TYPES: u32 = fnv1a("binTypes");
const BIN_FIELDS: u32 = fnv1a("binFields");
const BIN_HASHES: u32 = fnv1a("binHashes");
const GAME: u32 = fnv1a("game");

/// The names a bin is the only record of, one set per hash category.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathMap {
    pub bin_entries: BTreeSet<String>,
    pub bin_types: BTreeSet<String>,
    pub bin_fields: BTreeSet<String>,
    pub bin_hashes: BTreeSet<String>,
    pub game: BTreeSet<String>,
}

impl PathMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.sets().all(|set| set.is_empty())
    }

    pub fn len(&self) -> usize {
        self.sets().map(BTreeSet::len).sum()
    }

    pub fn merge(&mut self, other: &PathMap) {
        self.bin_entries.extend(other.bin_entries.iter().cloned());
        self.bin_types.extend(other.bin_types.iter().cloned());
        self.bin_fields.extend(other.bin_fields.iter().cloned());
        self.bin_hashes.extend(other.bin_hashes.iter().cloned());
        self.game.extend(other.game.iter().cloned());
    }

    /// The same names keyed by their hash, for a reader resolving a raw `0x…` back to a name.
    pub fn tables(&self) -> PathTables {
        PathTables {
            bin_entries: table32(&self.bin_entries),
            bin_types: table32(&self.bin_types),
            bin_fields: table32(&self.bin_fields),
            bin_hashes: table32(&self.bin_hashes),
            game: self
                .game
                .iter()
                .map(|path| (xxh64(path), path.clone()))
                .collect(),
        }
    }

    fn sets(&self) -> impl Iterator<Item = &BTreeSet<String>> {
        [
            &self.bin_entries,
            &self.bin_types,
            &self.bin_fields,
            &self.bin_hashes,
            &self.game,
        ]
        .into_iter()
    }
}

/// [`PathMap`] inverted into hash lookups, built once by [`PathMap::tables`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathTables {
    pub bin_entries: BTreeMap<u32, String>,
    pub bin_types: BTreeMap<u32, String>,
    pub bin_fields: BTreeMap<u32, String>,
    pub bin_hashes: BTreeMap<u32, String>,
    pub game: BTreeMap<u64, String>,
}

/// The record `bin` carries, empty when it carries none.
pub fn read_path_map(bin: &Bin) -> PathMap {
    let Some(entry) = bin
        .entries
        .iter()
        .find(|entry| entry.class_hash == RITOBINMAP_CLASS)
    else {
        return PathMap::new();
    };
    PathMap {
        bin_entries: strings(entry.fields.get(&BIN_ENTRIES)),
        bin_types: strings(entry.fields.get(&BIN_TYPES)),
        bin_fields: strings(entry.fields.get(&BIN_FIELDS)),
        bin_hashes: strings(entry.fields.get(&BIN_HASHES)),
        game: strings(entry.fields.get(&GAME)),
    }
}

/// Removes the record from `bin` and returns it.
pub fn strip_path_map(bin: &mut Bin) -> PathMap {
    let map = read_path_map(bin);
    bin.entries
        .retain(|entry| entry.class_hash != RITOBINMAP_CLASS);
    map
}

/// Replaces the record in `bin` with `map`, dropping it entirely when `map` is empty.
///
/// Also clears the legacy `CELMAP` footer: that one lives after the declared body, which
/// ritobin rejects outright (`bin_assert(reader.cur_ == reader.cap_)`), so a bin being
/// rewritten must not keep it.
pub fn write_path_map(bin: &mut Bin, map: &PathMap) {
    strip_path_map(bin);
    bin.trailing = crate::trailer::strip_trailer(&bin.trailing).to_vec();
    if map.is_empty() {
        return;
    }
    let mut fields = IndexMap::new();
    push_list(&mut fields, BIN_ENTRIES, &map.bin_entries);
    push_list(&mut fields, BIN_TYPES, &map.bin_types);
    push_list(&mut fields, BIN_FIELDS, &map.bin_fields);
    push_list(&mut fields, BIN_HASHES, &map.bin_hashes);
    push_list(&mut fields, GAME, &map.game);
    bin.entries.push(BinEntry {
        path_hash: RITOBINMAP_ENTRY,
        class_hash: RITOBINMAP_CLASS,
        fields,
    });
}

/// Files each of `names` under every category whose hashes `bin` actually uses, dropping
/// the rest.
///
/// This is the capture step: hand it the readable names a tool still has — the text it is
/// about to hash, or the values of a legacy [`crate::Trailer`] — and it keeps the ones the
/// bin would otherwise remember only as a hash. The record's own entry is skipped, so
/// capturing twice does not start naming the record itself.
pub fn capture<I, S>(bin: &Bin, names: I) -> PathMap
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let used = hash_uses(bin);
    let mut map = PathMap::new();
    for name in names {
        let name = name.as_ref();
        if name.is_empty() {
            continue;
        }
        let hash = fnv1a(name);
        if used.bin_entries.contains(&hash) {
            map.bin_entries.insert(name.to_string());
        }
        if used.bin_types.contains(&hash) {
            map.bin_types.insert(name.to_string());
        }
        if used.bin_fields.contains(&hash) {
            map.bin_fields.insert(name.to_string());
        }
        if used.bin_hashes.contains(&hash) {
            map.bin_hashes.insert(name.to_string());
        }
        if !used.game.is_empty() && used.game.contains(&xxh64(name)) {
            map.game.insert(name.to_string());
        }
    }
    map
}

#[derive(Default)]
struct HashUses {
    bin_entries: BTreeSet<u32>,
    bin_types: BTreeSet<u32>,
    bin_fields: BTreeSet<u32>,
    bin_hashes: BTreeSet<u32>,
    game: BTreeSet<u64>,
}

fn hash_uses(bin: &Bin) -> HashUses {
    let mut used = HashUses::default();
    for entry in &bin.entries {
        if entry.class_hash == RITOBINMAP_CLASS {
            continue;
        }
        used.bin_entries.insert(entry.path_hash);
        used.bin_types.insert(entry.class_hash);
        walk_fields(&entry.fields, &mut used);
    }
    for patch in &bin.patches {
        used.bin_entries.insert(patch.key_hash);
        walk_value(&patch.value, 0, &mut used);
    }
    used
}

fn walk_fields(fields: &IndexMap<u32, BinValue>, used: &mut HashUses) {
    for (field, value) in fields {
        used.bin_fields.insert(*field);
        walk_value(value, *field, used);
    }
}

fn walk_value(value: &BinValue, field: u32, used: &mut HashUses) {
    match value {
        BinValue::Hash(hash) => {
            used.bin_hashes.insert(*hash);
        }
        BinValue::Link(hash) => {
            used.bin_entries.insert(*hash);
        }
        BinValue::File(hash) if *hash != 0 => {
            used.game.insert(*hash);
        }
        BinValue::U64(packed) if crate::blend::is_blend_key_field(field) => {
            used.bin_hashes.insert((packed >> 32) as u32);
            used.bin_hashes.insert(*packed as u32);
        }
        BinValue::Pointer { class, fields } | BinValue::Embed { class, fields } => {
            used.bin_types.insert(*class);
            walk_fields(fields, used);
        }
        BinValue::List { items, .. } => {
            for item in items {
                walk_value(item, field, used);
            }
        }
        BinValue::Map { entries, .. } => {
            for (key, value) in entries {
                walk_value(key, field, used);
                walk_value(value, field, used);
            }
        }
        BinValue::Option {
            value: Some(value), ..
        } => walk_value(value, field, used),
        _ => {}
    }
}

fn table32(names: &BTreeSet<String>) -> BTreeMap<u32, String> {
    names
        .iter()
        .map(|name| (fnv1a(name), name.clone()))
        .collect()
}

fn push_list(fields: &mut IndexMap<u32, BinValue>, field: u32, names: &BTreeSet<String>) {
    if names.is_empty() {
        return;
    }
    fields.insert(
        field,
        BinValue::List {
            is_list2: false,
            item: BinType::String,
            items: names
                .iter()
                .map(|name| BinValue::String(name.clone()))
                .collect(),
        },
    );
}

fn strings(value: Option<&BinValue>) -> BTreeSet<String> {
    let Some(BinValue::List { items, .. }) = value else {
        return BTreeSet::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            BinValue::String(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bin() -> Bin {
        let mut fields = IndexMap::new();
        fields.insert(fnv1a("mEmitterName"), BinValue::Hash(fnv1a("MyEmitter")));
        fields.insert(
            fnv1a("texturePath"),
            BinValue::File(xxh64("ASSETS/Modders/Me/Custom.dds")),
        );
        fields.insert(
            fnv1a("mChild"),
            BinValue::Link(fnv1a("Characters/Me/Skins/Skin99")),
        );
        let mut bin = Bin::new();
        bin.entries.push(BinEntry {
            path_hash: fnv1a("Characters/Me/Skins/Skin99/Resources"),
            class_hash: fnv1a("VfxSystemDefinitionData"),
            fields,
        });
        bin
    }

    fn sample_map() -> PathMap {
        let mut map = PathMap::new();
        map.bin_entries.insert("Characters/Me/Skins/Skin99".into());
        map.bin_hashes.insert("MyEmitter".into());
        map.game.insert("ASSETS/Modders/Me/Custom.dds".into());
        map
    }

    #[test]
    fn round_trips_through_the_entry() {
        let mut bin = sample_bin();
        write_path_map(&mut bin, &sample_map());
        assert_eq!(read_path_map(&bin), sample_map());
    }

    #[test]
    fn writing_twice_leaves_one_record() {
        let mut once = sample_bin();
        write_path_map(&mut once, &sample_map());
        let mut twice = once.clone();
        write_path_map(&mut twice, &sample_map());
        assert_eq!(once.entries, twice.entries);
    }

    #[test]
    fn an_empty_map_writes_no_entry() {
        let mut bin = sample_bin();
        write_path_map(&mut bin, &sample_map());
        write_path_map(&mut bin, &PathMap::new());
        assert_eq!(bin.entries, sample_bin().entries);
    }

    #[test]
    fn writing_drops_the_legacy_footer() {
        let mut bin = sample_bin();
        bin.trailing = b"{\"deadbeef\":\"Name\"}\x13\x00\x00\x00CELMAP\0\0".to_vec();
        write_path_map(&mut bin, &sample_map());
        assert!(bin.trailing.is_empty());
    }

    #[test]
    fn stripping_returns_the_record_and_the_original_entries() {
        let mut bin = sample_bin();
        write_path_map(&mut bin, &sample_map());
        assert_eq!(strip_path_map(&mut bin), sample_map());
        assert_eq!(bin.entries, sample_bin().entries);
    }

    #[test]
    fn capture_files_a_name_under_the_category_that_uses_it() {
        let bin = sample_bin();
        let captured = capture(
            &bin,
            [
                "MyEmitter",
                "ASSETS/Modders/Me/Custom.dds",
                "Characters/Me/Skins/Skin99",
                "VfxSystemDefinitionData",
                "mEmitterName",
                "AName/Nothing/Uses",
            ],
        );
        assert_eq!(captured.bin_hashes, sample_map().bin_hashes);
        assert_eq!(captured.game, sample_map().game);
        assert_eq!(captured.bin_entries, sample_map().bin_entries);
        assert_eq!(
            captured.bin_types,
            BTreeSet::from(["VfxSystemDefinitionData".to_string()])
        );
        assert_eq!(
            captured.bin_fields,
            BTreeSet::from(["mEmitterName".to_string()])
        );
    }

    #[test]
    fn capture_ignores_the_record_it_already_wrote() {
        let mut bin = sample_bin();
        write_path_map(&mut bin, &sample_map());
        let captured = capture(
            &bin,
            [
                RITOBINMAP_ENTRY_NAME,
                RITOBINMAP_CLASS_NAME,
                "binEntries",
                "game",
            ],
        );
        assert!(captured.is_empty());
    }

    #[test]
    fn a_name_used_as_two_kinds_is_kept_under_both() {
        let mut bin = sample_bin();
        bin.entries[0].fields.insert(
            fnv1a("mSelf"),
            BinValue::Hash(fnv1a("Characters/Me/Skins/Skin99")),
        );
        let captured = capture(&bin, ["Characters/Me/Skins/Skin99"]);
        assert_eq!(captured.bin_entries.len(), 1);
        assert_eq!(captured.bin_hashes.len(), 1);
    }

    #[test]
    fn tables_key_each_category_by_its_own_hash() {
        let tables = sample_map().tables();
        assert_eq!(
            tables
                .bin_hashes
                .get(&fnv1a("MyEmitter"))
                .map(String::as_str),
            Some("MyEmitter")
        );
        assert_eq!(
            tables
                .game
                .get(&xxh64("ASSETS/Modders/Me/Custom.dds"))
                .map(String::as_str),
            Some("ASSETS/Modders/Me/Custom.dds")
        );
        assert!(
            tables
                .bin_entries
                .contains_key(&fnv1a("Characters/Me/Skins/Skin99"))
        );
        assert!(tables.bin_fields.is_empty());
    }

    #[test]
    fn a_bin_without_a_record_reads_empty() {
        assert!(read_path_map(&sample_bin()).is_empty());
    }
}
