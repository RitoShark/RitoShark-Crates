#![forbid(unsafe_code)]
/*!
rs_troybin reads and writes the legacy League `.troybin` particle/VFX parameter format. Values are
keyed by a 32-bit 65599 `ihash` of their `section/name` pair; the binary stores only hashes, so the
model keeps raw hashes as the source of truth and never applies the display multipliers the old
converters did. Version 2 lays values out in up to fourteen typed buckets selected by a flags word;
version 1 is a flat `(hash, offset)` table into a string blob. The reader preserves bucket order,
per-bucket hash order, raw typed values, and the version-1 blob verbatim, and the writer rebuilds
the file byte-for-byte. The format is the same one inibin v1/v2 uses.

On top of the raw model the crate owns the human-readable editing layer. [`ScalarValue`] flattens a
property out of its typed bucket so [`TroybinV2::get`]/[`set`](TroybinV2::set)/[`insert`](TroybinV2::insert)/
[`remove`](TroybinV2::remove) (and the `section`/`name` forms on [`Troybin`]) read and edit one
property without tracking bucket bits, packed booleans, or the strings blob — the string blob and
its offsets are recomputed automatically on edit, and `strings_length` is derived on write.
[`TroybinResolver`] turns raw hashes back into `section/name` for display. Editing targets v2; the
v1 body carries no value typing and is treated as read-only (`get` yields `None`, `set` errors).

`.inibin` and `.cfgbin` are the same format under different extensions, so the crate also exposes it
under that name: [`Inibin`] is [`Troybin`], [`Inibin::from_slice`] reads one, and [`InibinFlags`]
names the fourteen bucket bits plus the version-1 body so buckets can be enumerated by value type
rather than raw bit. The fixed-point bits stay raw `u8` on the model — [`fixed_point_to_f64`] and
[`fixed_point_from_f64`] are a derived display view, never the stored form.
*/

mod dict;
mod error;
mod inibin;
mod read;
mod resolve;
mod troybin;
mod write;

pub use error::{Error, Result};
pub use inibin::{
    FIXED_POINT_SCALE, Inibin, InibinFlags, fixed_point_from_f64, fixed_point_to_f64,
};
pub use resolve::{ResolvedName, TroybinResolver};
pub use troybin::{
    Bucket, BucketValues, ScalarValue, Troybin, TroybinBody, TroybinV1, TroybinV2, V1Entry,
};
