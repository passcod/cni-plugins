use std::{collections::HashMap, net::IpAddr};

use async_std::task::block_on;
use cni_plugin::{
	error::CniError,
	reply::{reply, DnsReply, IpamSuccessReply},
	Cni,
};
use log::{debug, error, info, warn};
use serde::Serialize;
use url::Url;

use crate::error::{AppError, AppResult};
use crate::nomad::Alloc;

mod error;
mod nomad;

fn main() {
	cni_plugin::install_logger("ipam-ds-nomad.log");
	debug!("{} (CNI IPAM delegate plugin) version {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

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
			info!(
				"ipam-ds-consul serving spec v{} for command=any",
				cni_version
			);

			let res: AppResult<IpamSuccessReply> = block_on(async move {
				let alloc_id = if container_id.starts_with("cnitool-") {
					"ae999124-a427-2e89-f763-a8742900854b".into() // testing
				} else {
					container_id
				};

				let ipam = config.ipam.clone().ok_or(CniError::MissingField("ipam"))?;
				debug!("ipam={:?}", ipam);

				let mut nomad_servers = ipam
					.specific
					.get("nomad_servers")
					.ok_or(CniError::MissingField("ipam.nomad_servers"))
					.and_then(|v| -> Result<Vec<Url>, _> {
						serde_json::from_value(v.to_owned()).map_err(CniError::Json)
					})?;
				debug!(
					"nomad-servers={}",
					nomad_servers
						.iter()
						.map(ToString::to_string)
						.collect::<Vec<String>>()
						.join(",")
				);

				nomad_servers.reverse();
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
						Ok(res) => {
							debug!("found good nomad server: {}", nomad_url);
							break res;
						}
						Err(err) => {
							if let Some(url) = nomad_servers.pop() {
								warn!("bad nomad server, trying next. err={}", err);
								nomad_url = url;
							} else {
								return Err(err);
							}
						}
					}
				};
				debug!("alloc={:?}", alloc);

				debug!("checking we have the group definition");
				let group = alloc.job.task_groups.iter().find(|g| g.name == alloc.task_group).ok_or(AppError::InvalidResource {
					remote: "nomad",
					resource: "allocation",
					path: alloc_id.clone(),
					err: Box::new(CniError::Generic(format!("alloc {} is for task group {} but its own job definition is missing it", alloc_id, alloc.task_group)))
				})?.clone();

				// TODO: enable this
				if false {
					debug!("checking group network is a cni network");
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

				debug!("reading pool name");
				let name = group
					.meta
					.network_pool
					.ok_or(CniError::MissingField("alloc.group.meta.network-pool"))?;
				info!("pool-name={}", name);

				debug!("reading requested ip");
				let requested_ip = group.meta.network_ip;
				info!("requested-ip={:?}", requested_ip);

				let mut specific = HashMap::new();
				specific.insert(
					"pools".into(),
					serde_json::to_value(&vec![Pool { name, requested_ip }])
						.map_err(CniError::Json)?,
				);

				let ips = if let Some(prev_ipam) =
					config
						.prev_result
						.and_then(|val| -> Option<IpamSuccessReply> {
							serde_json::from_value(val).ok()
						}) {
					prev_ipam.ips
				} else {
					Vec::new()
				};

				Ok(IpamSuccessReply {
					cni_version: config.cni_version,
					ips,
					routes: Vec::new(),
					dns: DnsReply::default(),
					specific,
				})

				// TODO: support multiple cni networks / multiple groups?
			});

			match res {
				Ok(res) => reply(res),
				Err(res) => {
					error!("error: {}", res);
					reply(res.into_result(cni_version))
				}
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
