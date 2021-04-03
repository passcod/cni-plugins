//! Library to write CNI plugins.
//!
//! - CNI information: on the [cni.dev](https://cni.dev) website.
//! - Tooling overview: https://github.com/passcod/cni-plugins/blob/main/docs/Standard-Tooling.md
//! - Tutorial: https://github.com/passcod/cni-plugins/blob/main/docs/Plugin-Hello-World.md
#![warn(missing_docs)]

pub use cni::Cni;
pub use command::Command;
pub use inputs::Inputs;
pub use logger::install_logger;

pub mod config;
#[cfg(any(feature = "with-smol", feature = "with-tokio"))]
pub mod delegation;
pub mod error;
pub mod ip_range;
pub mod reply;

mod cni;
mod command;
mod dns;
mod inputs;
mod logger;
mod path;
mod version;
