//! The async service facade over the db engine and local store, plus the pure
//! services (`values`, `csv`, `compare`, `mock`) the handlers build on.

pub mod compare;
pub mod csv;
pub mod mock;
pub mod values;

mod data;
mod facade;
mod generate;
mod query;
mod records;
mod tables;

pub use facade::Host;
