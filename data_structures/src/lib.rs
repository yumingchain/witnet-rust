// To enable `#[allow(clippy::all)]`
//#![feature(tool_lints)]

#![cfg_attr(test, allow(dead_code, unused_macros, unused_imports))]

#[macro_use]
extern crate serde_derive;

/// Module containing functions to generate witnet's protocol messages
pub mod builders;

/// Module generated by flatbuffers compiler, containing flatbuffers protocol messages types
pub mod flatbuffers;

/// Module containing functions to cast witnet's protocol messages to flatbuffers and vice versa
pub mod serializers;

/// Module containing witnet's protocol messages types
pub mod types;

/// Module containing ChainInfo data structure
pub mod chain;
