#![warn(missing_docs)]

//! Library to write CNI plugins.
//!
//! - CNI information: on the [cni.dev](https://cni.dev) website.
//! - [Tooling overview][tools]
//! - [Tutorial][tuto]
//!
//! [tools]: https://github.com/passcod/cni-plugins/blob/main/docs/Standard-Tooling.md
//! [tuto]: https://github.com/passcod/cni-plugins/blob/main/docs/Plugin-Hello-World.md
//!
//! # Quick start
//!
//! ```no_run
//! use cni_plugin::{Cni, install_logger};
//! install_logger("hello-world.log");
//! match Cni::load() {
//!     Cni::Add { container_id, ifname, netns, path, config } => {}
//!     Cni::Del { container_id, ifname, netns, path, config } => {}
//!     Cni::Check { container_id, ifname, netns, path, config } => {}
//!     Cni::Version(_) => unreachable!()
//! }
//! ```

pub use cni::Cni;
pub use command::Command;
pub use inputs::Inputs;
pub use logger::install_logger;

pub mod config;
#[cfg(any(feature = "with-smol", feature = "with-tokio"))]
pub mod delegation;
pub mod error;
pub mod ip_range;
pub mod macaddr;
pub mod reply;

mod cni;
mod command;
mod dns;
mod inputs;
mod logger;
mod path;
mod version;
