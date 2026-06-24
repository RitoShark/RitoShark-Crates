#![forbid(unsafe_code)]
/*!
The `rst list` command prints every `(hash, value)` entry in a string table — the hex hash and
its UTF-8 value, or `<encrypted>` for an encrypted blob — as text or JSON.
*/

use std::path::Path;

use crate::error::Result;

pub fn list(input: &Path, json: bool) -> Result<()> {
    use ritoshark::prelude::*;
    let rst = ritoshark::rst::Rst::from_path(input)?;
    if json {
        let arr: Vec<_> = rst
            .entries
            .iter()
            .map(|(h, v)| serde_json::json!({ "hash": format!("{h:010x}"), "value": v.as_str() }))
            .collect();
        println!(
            "{}",
            serde_json::json!({ "version": rst.version, "entries": arr })
        );
    } else {
        for (h, v) in &rst.entries {
            match v.as_str() {
                Some(s) => println!("{h:010x}  {s}"),
                None => println!("{h:010x}  <encrypted>"),
            }
        }
    }
    Ok(())
}
