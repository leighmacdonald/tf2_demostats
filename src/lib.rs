extern crate core;

pub mod parser;
pub mod schema;
pub mod web;

pub type Result<T> = core::result::Result<T, Error>;
pub type Error = Box<dyn std::error::Error>;
