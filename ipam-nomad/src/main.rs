use std::{collections::HashMap, net::IpAddr};

use async_std::task::block_on;
use cni_plugin::{
	error::CniError,
	reply::{reply, DnsReply, IpamSuccessReply},
	Cni,
};
use serde::Serialize;
use serde_json::Value;
use url::Url;

use crate::error::{AppError, AppResult};
use crate::nomad::Alloc;

mod error;
mod nomad;

fn main() {
	cni_plugin::install_logger("ipam-nomad.log");
	match Cni::load() {
		Cni::Add {
			container_id,
			config,
			..
		}
		| Cni::Del {
			container_id,
			config,
			..
		}
		| Cni::Check {
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

				let mut nomad_servers = ipam
					.specific
					.get("nomad_servers")
					.ok_or(CniError::MissingField("ipam.nomad_servers"))
					.and_then(|v| -> Result<Vec<Url>, _> {
						serde_json::from_value(v.to_owned()).map_err(CniError::Json)
					})?;

				let mut nomad_url = nomad_servers
					.pop()
					.ok_or(CniError::MissingField("ipam.nomad_servers"))?;
				let alloc: Alloc = loop {
					match surf::get(nomad_url.join("v1/allocation/")?.join(&alloc_id)?)
						.recv_json()
						.await
						.map_err(|err| AppError::Fetch {
							remote: "nomad",
							resource: "allocation",
							err: err.into(),
						}) {
						Ok(res) => break res,
						Err(err) => {
							if let Some(url) = nomad_servers.pop() {
								nomad_url = url;
							} else {
								return Err(err);
							}
						}
					}
				};

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

				let mut specific = HashMap::new();
				specific.insert(
					"pools".into(),
					serde_json::to_value(&vec![Pool {
						name: pool_name,
						requested_ip: group.meta.network_ip,
					}])
					.map_err(CniError::Json)?,
				);

				Ok(IpamSuccessReply {
					cni_version: config.cni_version,
					ips: Vec::new(),
					routes: Vec::new(),
					dns: DnsReply::default(),
					specific,
				})

				// TODO: support multiple cni networks / multiple groups?
			});

			match res {
				Ok(res) => reply(res),
				Err(res) => reply(res.into_result(cni_version)),
			}
		}
		Cni::Version(_) => unreachable!(),
	}
}

#[derive(Clone, Debug, Serialize)]
struct Pool {
	name: String,
	requested_ip: Option<IpAddr>,
}
