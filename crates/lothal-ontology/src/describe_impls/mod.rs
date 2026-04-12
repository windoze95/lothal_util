//! `impl Describe for X` blocks for every meaningful domain type in
//! `lothal-core`.
//!
//! These impls live here (not in `lothal-core`) because `lothal-core` does
//! not depend on this crate — the dependency arrow points the other way, and
//! the `Describe` trait is defined here in [`crate::object`].

mod site;
mod structure;
mod device;
mod circuit;
mod utility_account;
mod bill;
mod flock;
mod garden_bed;
mod pool;
mod property_zone;
mod experiment;
mod maintenance;
