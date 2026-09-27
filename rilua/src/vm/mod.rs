//! Virtual machine: bytecode execution and runtime state.

pub mod callinfo;
pub mod closure;
pub mod debug_info;
pub mod dump;
pub mod execute;
pub mod gc;
pub mod instructions;
pub mod listing;
pub mod metatable;
pub mod proto;
pub mod state;
pub mod string;
pub mod table;
pub mod undump;
pub mod value;
