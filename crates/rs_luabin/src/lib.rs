#![forbid(unsafe_code)]
/*!
rs_luabin reads and writes League `.luabin64` files: compiled Lua 5.1 bytecode with the 64-bit
`size_t` layout Riot's data files use. It parses the chunk into a fully-addressable structural model
— the global header and the recursive function-prototype tree, including instructions, the constant
pool, nested prototypes, and the debug tables — keeping every byte so `read -> write` reproduces the
file exactly. It deliberately does not decompile or recompile Lua source; editing instead happens on
the constant pool, where [`LuaConstant`] exposes typed get/set for numbers and strings so a literal
(a damage value, a label) can be changed and the chunk re-emitted with only that constant altered.

On top of that raw model the crate owns the human-readable layer an editor needs: [`LuaBin::iter_constants`]
walks the whole prototype tree and addresses every constant — including those buried in nested
functions — by a stable [`ConstPath`], and [`LuaBin::global_assignments`] reconstructs the
`GlobalName = value` pairings from the bytecode so a value can be shown and edited under its name
rather than as a blind constant index.
*/

mod error;
mod globals;
mod luabin;
mod read;
mod write;

pub use error::{Error, Result};
pub use luabin::{ConstPath, GlobalAssignment, LocalVar, LuaBin, LuaConstant, Proto};
