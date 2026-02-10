#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![forbid(unsafe_code)]

pub mod config;
pub mod entities;
pub mod events;
mod framework;
pub mod processors;
pub mod utils;
