/// XXH64 (seed 0) over the lowercased ASCII bytes of `s`, as used for `File` values and WAD paths.
pub fn xxh64(s: &str) -> u64 {
    xxhash_rust::xxh64::xxh64(s.to_ascii_lowercase().as_bytes(), 0)
}

/// xxh3-64 over the lowercased ASCII bytes of `s`, as used for RST string-table keys.
pub fn xxh3_64(s: &str) -> u64 {
    xxhash_rust::xxh3::xxh3_64(s.to_ascii_lowercase().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive() {
        assert_eq!(xxh64("Common/Stuff"), xxh64("common/stuff"));
        assert_eq!(xxh3_64("Common/Stuff"), xxh3_64("common/stuff"));
    }

    #[test]
    fn distinct_inputs_differ() {
        assert_ne!(xxh64("a"), xxh64("b"));
        assert_ne!(xxh3_64("a"), xxh3_64("b"));
    }
}
