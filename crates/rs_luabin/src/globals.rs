/*!
Reconstructs `GlobalName = value` assignments from the bytecode. Lua 5.1 compiles a global
assignment of a literal as a `LOADK` of the value into a register immediately followed by a
`SETGLOBAL` storing that register under a string-constant key; matching that adjacent pair across
every prototype recovers the name ↔ constant pairing the raw constant pool alone cannot express.
*/

use crate::luabin::{ConstPath, GlobalAssignment, LuaBin, LuaConstant, Proto};

const OP_LOADK: u32 = 1;
const OP_SETGLOBAL: u32 = 7;

pub(crate) fn global_assignments(bin: &LuaBin) -> Vec<GlobalAssignment> {
    let mut out = Vec::new();
    walk(&bin.main, &mut Vec::new(), &mut out);
    out
}

fn walk(proto: &Proto, stack: &mut Vec<usize>, out: &mut Vec<GlobalAssignment>) {
    let code = &proto.code;
    for i in 1..code.len() {
        let instr = code[i];
        let prev = code[i - 1];
        if opcode(instr) != OP_SETGLOBAL || opcode(prev) != OP_LOADK || reg_a(instr) != reg_a(prev)
        {
            continue;
        }
        let value_index = bx(prev) as usize;
        if value_index >= proto.constants.len() {
            continue;
        }
        let Some(name) = string_const(proto, bx(instr) as usize) else {
            continue;
        };
        out.push(GlobalAssignment {
            name,
            value: ConstPath {
                proto: stack.clone(),
                index: value_index,
            },
        });
    }

    for (j, child) in proto.protos.iter().enumerate() {
        stack.push(j);
        walk(child, stack, out);
        stack.pop();
    }
}

fn opcode(instr: u32) -> u32 {
    instr & 0x3F
}

fn reg_a(instr: u32) -> u32 {
    (instr >> 6) & 0xFF
}

fn bx(instr: u32) -> u32 {
    (instr >> 14) & 0x3_FFFF
}

fn string_const(proto: &Proto, index: usize) -> Option<String> {
    match proto.constants.get(index)? {
        c @ LuaConstant::Str(_) => c
            .as_string()
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}
