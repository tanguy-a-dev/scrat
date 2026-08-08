//! Scrat domain layer: entities, value objects, and repository ports.
//! Pure Rust — no I/O, no async, no framework dependencies. Everything here
//! must be testable with plain `cargo test`, no database or Tauri runtime.

pub mod account;
pub mod category;
pub mod default_categories;
pub mod language;
pub mod money;
pub mod ports;
pub mod recurring;
pub mod transaction;
pub mod transfer_rule;
