/*!
Turns the raw 32-bit property hashes back into readable `section/name` pairs. Because the format
keys values by `ihash(section/name)` and stores only the hash, resolution is a guess-and-check: the
candidate names from [`dict`](crate::dict) are hashed under each section and matched to the hashes
present in the file. Group, field, and fluid section names are not fixed — they are themselves
string values the System section points at — so [`TroybinResolver::build`] first reads those
pointers out of the parsed file, then hashes the per-kind dictionaries under the discovered sections.
*/

use std::collections::HashMap;

use crate::dict;
use crate::troybin::{ScalarValue, Troybin, TroybinBody};

/// A hash resolved back to the `section` and property `name` that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedName {
    pub section: String,
    pub name: String,
}

/// A hash→name index built from one parsed file. Construct it once with [`TroybinResolver::build`]
/// (or [`Troybin::resolver`](crate::Troybin::resolver)) and query it for as many hashes as needed.
#[derive(Debug, Clone, Default)]
pub struct TroybinResolver {
    map: HashMap<u32, ResolvedName>,
}

impl TroybinResolver {
    /// Builds the index from a parsed file: discovers the group/field/fluid section names the System
    /// section points at, then hashes every candidate name under each section. A v1 body resolves to
    /// an empty index, since v1 stores no value typing to follow.
    pub fn build(troybin: &Troybin) -> Self {
        let mut map = HashMap::new();
        if !matches!(troybin.body, TroybinBody::V2(_)) {
            return TroybinResolver { map };
        }

        let groups = discover(troybin, &["System".to_string()], &dict::part_group_names());
        let fields = discover(troybin, &groups, &dict::part_field_names());
        let fluids = discover(troybin, &groups, &dict::part_fluid_names());

        add(&mut map, &groups, &dict::group_names());
        add(&mut map, &fields, &dict::field_names());
        add(&mut map, &fluids, &dict::fluid_names());
        add(&mut map, &["System".to_string()], &dict::system_names());

        TroybinResolver { map }
    }

    /// The full `section`/`name` resolution for `hash`, or `None` if no candidate matched.
    pub fn resolve(&self, hash: u32) -> Option<&ResolvedName> {
        self.map.get(&hash)
    }

    /// Just the property name for `hash` (the common display case).
    pub fn name(&self, hash: u32) -> Option<&str> {
        self.map.get(&hash).map(|r| r.name.as_str())
    }

    /// Number of distinct hashes the index can resolve.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl Troybin {
    /// Builds a [`TroybinResolver`] from this file. See [`TroybinResolver::build`].
    pub fn resolver(&self) -> TroybinResolver {
        TroybinResolver::build(self)
    }
}

fn discover(troybin: &Troybin, sections: &[String], names: &[String]) -> Vec<String> {
    let TroybinBody::V2(body) = &troybin.body else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for section in sections {
        for name in names {
            let hash = Troybin::property_hash(section, name);
            if let Some(ScalarValue::String(bytes)) = body.get(hash)
                && !bytes.is_empty()
            {
                out.push(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
    }
    out
}

fn add(map: &mut HashMap<u32, ResolvedName>, sections: &[String], names: &[String]) {
    for section in sections {
        for name in names {
            let hash = Troybin::property_hash(section, name);
            map.entry(hash).or_insert_with(|| ResolvedName {
                section: section.clone(),
                name: name.clone(),
            });
        }
    }
}
