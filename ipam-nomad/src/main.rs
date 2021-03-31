use async_std::task::block_on;
use cni_plugin::{
	error::CniError,
	reply::{reply, IpamSuccessReply},
	Cni,
};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::nomad::Alloc;

mod error;
mod nomad;

fn main() {
	match Cni::load() {
		Cni::Add {
			container_id,
			config,
			..
		} => {
			let cni_version = config.cni_version.clone(); // for error
			let res: AppResult<IpamSuccessReply> = block_on(async move {
				let alloc_id = if container_id.starts_with("cnitool-") {
					"d3428f56-9480-d309-6343-4ec7feded3b3".into() // testing
				} else {
					container_id
				};

				let ipam = config.ipam.clone().ok_or(CniError::MissingField("ipam"))?;

				let get_config = |name: &'static str| -> Result<&Value, CniError> {
					ipam.specific
						.get(name)
						.ok_or(CniError::MissingField("ipam"))
				};

				let config_string = |name: &'static str| -> Result<String, CniError> {
					get_config(name).and_then(|v| {
						if let Value::String(s) = v {
							Ok(s.to_owned())
						} else {
							Err(CniError::InvalidField {
								field: name,
								expected: "string",
								value: v.clone(),
							})
						}
					})
				};

				let nomad_url = config_string("nomad_url")?;

				let alloc: Alloc = surf::get(format!("{}/v1/allocation/{}", nomad_url, alloc_id))
					.recv_json()
					.await
					.map_err(|err| AppError::Fetch {
						remote: "nomad",
						resource: "allocation",
						err: err.into(),
					})?;

				let group = alloc.job.task_groups.iter().find(|g| g.name == alloc.task_group).ok_or(AppError::InvalidResource {
                    remote: "nomad",
                    resource: "allocation",
                    path: alloc_id.clone(),
                    err: Box::new(CniError::Generic(format!("alloc {} is for task group {} but its own job definition is missing it", alloc_id, alloc.task_group)))
                })?.clone();

				// TODO: enable this
				if false {
					if let Some(network_mode) = group.networks.first().map(|n| &n.mode) {
						if !network_mode.starts_with("cni/") {
							return Err(CniError::InvalidField {
								field: "alloc.group.networks[0].mode",
								expected: "cni/<name>",
								value: network_mode.as_str().into(),
							}
							.into());
						}
					} else {
						return Err(CniError::MissingField("alloc.group.networks[0]").into());
					}
				}

				let pool_name = group
					.meta
					.network_pool
					.ok_or(CniError::MissingField("alloc.group.meta.network-pool"))?;

				let requested_ip = group.meta.network_ip;

				// TODO: support multiple cni networks / multiple groups?
				// return ipam result with pool name and requested ip

				Err(CniError::Debug(Box::new((pool_name, requested_ip))).into())
			});

			match res {
				Ok(res) => reply(res),
				Err(res) => reply(res.into_result(cni_version)),
			}
		}
		Cni::Del {
			container_id,
			config,
			..
		} => {}
		Cni::Check {
			container_id,
			config,
			..
		} => {}
		Cni::Version(_) => unreachable!(),
	}
}
