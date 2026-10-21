//! Turns name-level references into resolved edges: module specifiers to
//! files, then names to symbols through imports, scope and type hints.

pub mod modules;

pub use modules::{ModuleResolver, ModuleTarget};
