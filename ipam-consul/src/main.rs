use std::{collections::BTreeMap, net::IpAddr, str::FromStr};

use async_std::task::block_on;
use cni_plugin::{
	ip_range::IpRange,
	reply::{reply, IpamSuccessReply},
	Cni,
};
use consul::ConsulValue;
use serde::Deserialize;
use serde_json::Value;

use crate::consul::ConsulPair;
use crate::error::{AppError, AppResult, OtherErr};
use crate::nomad::Alloc;

mod consul;
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

				let ipam = config.ipam.clone().ok_or(AppError::MissingField("ipam"))?;

				let get_config = |name: &'static str| -> AppResult<&Value> {
					ipam.specific
						.get(name)
						.ok_or(AppError::MissingField("ipam"))
				};

				let config_string = |name: &'static str| -> AppResult<String> {
					get_config(name).and_then(|v| {
						if let Value::String(s) = v {
							Ok(s.to_owned())
						} else {
							Err(AppError::InvalidFieldType {
								field: name,
								expected: "string",
								value: v.clone(),
							})
						}
					})
				};

				let pool_name = config_string("pool")?;
				let consul_url = config_string("consul_url")?;
				let nomad_url = config_string("nomad_url")?;

				// lookup defined pool in consul kv at ipam/<pool name>/
				// error if not found
				// parse as JSON Vec<cni::IpRange>
				let pool = {
					let keys: Vec<ConsulPair<Vec<IpRange>>> =
						surf::get(format!("{}/v1/kv/ipam/{}", consul_url, pool_name))
							.recv_json()
							.await
							.map_err(|err| AppError::Fetch {
								remote: "consul",
								resource: "pool name",
								err: err.into(),
							})?;

					keys.into_iter()
						.next()
						.ok_or(AppError::MissingResource {
							remote: "consul",
							resource: "pool",
							path: format!("ipam/{}", pool_name),
						})?
						.parsed_value()
						.map_err(|err| AppError::InvalidResource {
							remote: "consul",
							resource: "pool",
							path: format!("ipam/{}", pool_name),
							err: Box::new(err),
						})?
						.ok_or(AppError::InvalidResource {
							remote: "consul",
							resource: "pool",
							path: format!("ipam/{}", pool_name),
							err: OtherErr::boxed("expected IpRange as JSON, got null"),
						})?
				};

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
                    err: OtherErr::boxed(format!("alloc {} is for task group {} but its own job definition is missing it", alloc_id, alloc.task_group))
                })?.clone();

				// TODO: enable this
				if false {
					if let Some(network_mode) = group.networks.first().map(|n| &n.mode) {
						if !network_mode.starts_with("cni/") {
							return Err(AppError::InvalidFieldType {
								field: "alloc.group.networks[0].mode",
								expected: "cni/<name>",
								value: network_mode.as_str().into(),
							});
						}
					} else {
						return Err(AppError::MissingField("alloc.group.networks[0]"));
					}
				}

				let mut ip = config
					.runtime
					.as_ref()
					.map(|c| c.ips.first().map(|ip| ip.ip()))
					.flatten();

				if ip.is_none() {
					ip = group
						.meta
						.get("network-ip")
						.map(|v| {
							if let Value::String(s) = v {
								IpAddr::from_str(&s).map_err(|_| AppError::InvalidFieldType {
									field: "alloc.group.meta.network-ip",
									expected: "IP address",
									value: v.clone(),
								})
							} else {
								Err(AppError::InvalidFieldType {
									field: "alloc.group.meta.network-ip",
									expected: "string",
									value: v.clone(),
								})
							}
						})
						.transpose()?;
				}

				// if let Some(ip) = ip {
				//     if !(pool.subnets...).contains(ip) {
				//         return Err(AppError::TODO {
				//             // Requested IP not in pool
				//             format!(
				//                 "pool {} does not contain requested address {}",
				//                 pool_name, ip
				//             ),
				//         });
				//     }
				// }

				// let pool_known = fetch and parse {consul_url}/v1/kv/ipam/{pool_name}/?recurse
				let pool_known: Vec<ConsulPair<PoolEntry>> =
					surf::get(format!("{}/v1/kv/ipam/{}/?recurse", consul_url, pool_name))
						.recv_json()
						.await
						.map_err(|err| AppError::Fetch {
							remote: "consul",
							resource: "ip-pool",
							err: err.into(),
						})?;
				let pool_known: BTreeMap<IpAddr, String> = pool_known
					.into_iter()
					.filter(|pair| !pair.value.is_null())
					.map(|pair| {
						let key = pair.key.clone(); // for errors
						pair.parse_value()
							.map_err(|err| AppError::InvalidResource {
								remote: "consul",
								resource: "ip-pool",
								path: key.clone(),
								err: OtherErr::boxed(format!(
									"expected value to be a JSON string; {}",
									err
								)),
							})
							.and_then(|pair| {
								pair.key
									.split('/')
									.last()
									.ok_or_else(|| {
										unreachable!("due to how the key is constructed it will always have at least one segment")
									})
									.and_then(|ip| {
										IpAddr::from_str(ip).map_err(|err| {
											AppError::InvalidResource {
												remote: "consul",
												resource: "ip-pool",
												path: key.clone(),
												err: OtherErr::boxed(format!(
													"expected key to be an IP address; {}",
													err
												)),
											}
										})
									})
									.map(|ip| {
										(
											ip,
											if let ConsulValue::Parsed(v) = pair.value {
												v.target
											} else {
												unreachable!("consul value should be parsed and nulls already filtered")
											},
										)
									})
							})
					})
					.collect::<AppResult<BTreeMap<_, _>>>()?;

				eprintln!("{:?}", pool_known);

				// if no ip, fetch the list under the consul kv and pick the next one
				let next_ip = pool
					.iter()
					.flat_map(|range| range.iter_free())
					.filter(|ip| !pool_known.contains_key(&ip.ip()))
					.next()
					.ok_or(AppError::PoolFull(pool_name))?;
				// assign the container_id to the ip (if new/random ip, use cas=0)
				// if assign fails (ie another cni got the ip), retry up to 3 times

				// if no space in subnet, error

				// return ipam result

				Err(AppError::Debug(Box::new((
					pool,
					ip,
					group.networks,
					pool_known,
					next_ip,
				))))
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoolEntry {
	pub target: String,
}
