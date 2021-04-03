use std::path::PathBuf;

use crate::{Cni, Command, config::NetworkConfig};

#[derive(Clone, Debug)]
pub struct Inputs {
	pub command: Command,
	pub container_id: String,
	pub ifname: String,
	pub netns: Option<PathBuf>,
	pub path: Vec<PathBuf>,
	pub config: NetworkConfig,
}

impl Cni {
	pub fn into_inputs(self) -> Option<Inputs> {
		let command = match &self {
			Cni::Add { .. } => Command::Add,
			Cni::Del { .. } => Command::Del,
			Cni::Check { .. } => Command::Check,
			Cni::Version(_) => return None,
		};

		match self {
			Cni::Add {
				container_id,
				ifname,
				netns,
				path,
				config,
			}
			| Cni::Check {
				container_id,
				ifname,
				netns,
				path,
				config,
			} => Some(Inputs {
				command,
				container_id,
				ifname,
				netns: Some(netns),
				path,
				config,
			}),
			Cni::Del {
				container_id,
				ifname,
				netns,
				path,
				config,
			} => Some(Inputs {
				command,
				container_id,
				ifname,
				netns,
				path,
				config,
			}),
			Cni::Version(_) => unreachable!(),
		}
	}
}
