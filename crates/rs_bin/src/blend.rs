use rs_hash::fnv1a;

/** A clip-to-clip transition key. `mBlendDataTable` maps a transition to its blend data, and the
`u64` key is not a number but two FNV1a-32 clip-name hashes packed into one word: the clip being
blended out of in the high 32 bits, the clip being blended into in the low 32 bits. Splitting is
arithmetic, so a half with leading zero bytes is as safe as any other. */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlendKey {
    pub from: u32,
    pub to: u32,
}

impl BlendKey {
    pub const fn from_u64(key: u64) -> Self {
        Self {
            from: (key >> 32) as u32,
            to: key as u32,
        }
    }

    pub const fn to_u64(self) -> u64 {
        ((self.from as u64) << 32) | self.to as u64
    }

    pub const fn from_names(from: &str, to: &str) -> Self {
        Self {
            from: fnv1a(from),
            to: fnv1a(to),
        }
    }
}

/// FNV1a-32 of `mBlendDataTable`.
pub const BLEND_DATA_TABLE: u32 = fnv1a("mBlendDataTable");

/// Field-name hashes whose `map[u64, …]` keys are packed [`BlendKey`]s rather than plain numbers.
pub const BLEND_KEY_FIELDS: &[u32] = &[BLEND_DATA_TABLE];

pub fn is_blend_key_field(field: u32) -> bool {
    BLEND_KEY_FIELDS.contains(&field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_high_source_and_low_destination() {
        let key = BlendKey::from_u64(6247030502030953662);
        assert_eq!(key.from, fnv1a("Attack1"));
        assert_eq!(key.to, fnv1a("Laugh"));
    }

    #[test]
    fn packs_back_to_the_same_word() {
        for raw in [
            6247030502030953662u64,
            2432597616124873918,
            0,
            u64::MAX,
            0x0000_00ab_0000_00cd,
            0x0000_0000_ffff_ffff,
            0xffff_ffff_0000_0000,
        ] {
            assert_eq!(BlendKey::from_u64(raw).to_u64(), raw);
        }
    }

    #[test]
    fn leading_zero_halves_survive() {
        let key = BlendKey {
            from: 0x0000_00ab,
            to: 0x0000_00cd,
        };
        assert_eq!(key.to_u64(), 0x0000_00ab_0000_00cd);
        assert_eq!(BlendKey::from_u64(key.to_u64()), key);
    }

    #[test]
    fn names_hash_case_insensitively() {
        assert_eq!(
            BlendKey::from_names("Attack1", "Laugh"),
            BlendKey::from_names("attack1", "LAUGH")
        );
        assert_eq!(
            BlendKey::from_names("Attack1", "Laugh").to_u64(),
            6247030502030953662
        );
    }

    #[test]
    fn recognises_the_blend_table_field() {
        assert!(is_blend_key_field(fnv1a("mBlendDataTable")));
        assert!(is_blend_key_field(fnv1a("mblenddatatable")));
        assert!(!is_blend_key_field(fnv1a("mClipDataMap")));
    }
}
