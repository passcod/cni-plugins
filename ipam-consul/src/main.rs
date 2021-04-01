use std::{collections::BTreeMap, net::IpAddr, str::FromStr};

use async_std::task::block_on;
use cni_plugin::{
	error::CniError,
	ip_range::IpRange,
	reply::{reply, IpReply, IpamSuccessReply},
	Cni,
};
use consul::ConsulValue;
use ipnetwork::IpNetwork;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::consul::ConsulPair;
use crate::error::{AppError, AppResult};

mod consul;
mod error;

fn main() {
	cni_plugin::install_logger("ipam-consul.log");
	match Cni::load() {
		Cni::Add {
			container_id,
			config,
			..
		} => {
			let cni_version = config.cni_version.clone(); // for error
			info!("ipam-consul serving spec v{} command=Add", cni_version);

			let res: AppResult<IpamSuccessReply> = block_on(async move {
				let ipam = config.ipam.clone().ok_or(CniError::MissingField("ipam"))?;
				debug!("ipam={:?}", ipam);

				let prev_result: Option<IpamSuccessReply> = config
					.prev_result
					.map(|p| serde_json::from_value(p).map_err(CniError::Json))
					.transpose()?;
				debug!("prevResult={:?}", prev_result);

				let pools: Vec<Pool> = prev_result
					.map(|p| p.specific.get("pools").cloned())
					.flatten()
					.map(|p| serde_json::from_value(p).map_err(CniError::Json))
					.transpose()?
					.unwrap_or_default();
				debug!("pools={:?}", pools);
				// TODO: support multiple

				let selected_pool = pools.first().cloned().ok_or(AppError::MissingResource {
					remote: "prevResult",
					resource: "pool",
					path: "pools[0]".into(),
				})?;
				let pool_name = selected_pool.name;
				debug!(
					"pool name={} requested-ip={:?}",
					pool_name, selected_pool.requested_ip
				);

				let mut consul_servers = ipam
					.specific
					.get("consul_servers")
					.ok_or(CniError::MissingField("ipam.consul_servers"))
					.and_then(|v| -> Result<Vec<Url>, _> {
						serde_json::from_value(v.to_owned()).map_err(CniError::Json)
					})?;
				debug!(
					"consul-servers={}",
					consul_servers
						.iter()
						.map(ToString::to_string)
						.collect::<Vec<String>>()
						.join(",")
				);

				consul_servers.reverse();
				let mut consul_url = consul_servers
					.pop()
					.ok_or(CniError::MissingField("ipam.consul_servers"))?;

				let keys: Vec<ConsulPair<Vec<IpRange>>> = loop {
					match surf::get(consul_url.join("v1/kv/ipam/")?.join(&pool_name)?)
						.recv_json()
						.await
						.map_err(|err| AppError::Http {
							remote: "consul",
							resource: "pool name",
							err: err.into(),
						}) {
						Ok(res) => {
							debug!("found good consul server: {}", consul_url);
							break res;
						}
						Err(err) => {
							if let Some(url) = consul_servers.pop() {
								warn!("bad consul server, trying next. err={}", err);
								consul_url = url;
							} else {
								return Err(err);
							}
						}
					}
				};

				// lookup defined pool in consul kv at ipam/<pool name>/
				// error if not found
				// parse as JSON Vec<cni::IpRange>
				let pool = keys
					.into_iter()
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
						err: Box::new(CniError::Generic(
							"expected IpRange as JSON, got null".into(),
						)),
					})?;
				debug!("pool={:?}", pool);

				// let pool_known = fetch and parse {consul_url}/v1/kv/ipam/{pool_name}/?recurse
				let mut pool_url = consul_url.join("v1/kv/ipam/")?.join(&pool_name)?;
				pool_url.set_query(Some("recurse"));
				let pool_known: Vec<ConsulPair<PoolEntry>> = surf::get(pool_url)
					.recv_json()
					.await
					.map_err(|err| AppError::Http {
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
								err: Box::new(CniError::Generic(format!(
									"expected value to be a JSON string; {}",
									err
								))),
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
												err: Box::new(CniError::Generic(format!(
													"expected key to be an IP address; {}",
													err
												))),
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
				debug!("pool-known={:?}", pool_known);

				let (ip, gateway) = if let Some(ip) = selected_pool.requested_ip {
					debug!("checking whether requested ip fits in the selected pool");

					let mut prefix = None;
					let mut gateway = None;
					for range in &pool {
						if range.subnet.contains(ip) {
							prefix = Some(range.subnet.prefix());
							gateway = range.gateway;
						}
					}

					let prefix = prefix.ok_or(AppError::NotInPool {
						pool: pool_name.clone(),
						ip,
					})?;

					// UNWRAP: panics on invalid prefix, but prefix comes from existing IpNetwork
					(IpNetwork::new(ip, prefix).unwrap(), gateway)
				} else {
					debug!("none requested, picking next ip in pool");
					pool.iter()
						.flat_map(|range| range.iter_free())
						.filter(|(ip, _)| !pool_known.contains_key(&ip.ip()))
						.next()
						.map(|(ip, range)| (ip, range.gateway.clone()))
						.ok_or(AppError::PoolFull(pool_name.clone()))?
				};

				debug!("ip={:?}", ip);

				// assign the container_id to the ip (if new/random ip, use cas=0)
				let mut assign_url =
					consul_url.join(&format!("v1/kv/ipam/{}/{}", pool_name, ip))?;

				if selected_pool.requested_ip.is_none() {
					debug!("creating address"); // cas=0 ensures that it will fail if it's an update
					assign_url.query_pairs_mut().append_pair("cas", "0");
				}

				let success: bool = surf::put(assign_url)
					.body(
						serde_json::to_value(PoolEntry {
							target: container_id,
						})
						.map_err(CniError::Json)?,
					)
					.recv_json()
					.await
					.map_err(|err| AppError::Http {
						remote: "consul",
						resource: "pool entry",
						err: err.into(),
					})?;

				if success {
					info!("allocated address {}", ip);
					Ok(IpamSuccessReply {
						cni_version: config.cni_version,
						ips: vec![IpReply {
							address: ip,
							gateway,
							interface: None,
						}],
						routes: Vec::new(),
						dns: Default::default(),
						specific: Default::default(),
					})
				} else {
					error!("consul write to ipam/{}/{} returned false", pool_name, ip);
					Err(AppError::ConsulWriteFailed)
				}
			});

			match res {
				Ok(res) => {
					debug!("success! {:#?}", res);
					reply(res)
				}
				Err(res) => {
					error!("error: {}", res);
					reply(res.into_result(cni_version))
				}
			}
		}
		Cni::Del {
			container_id,
			config,
			..
		} => {
			let cni_version = config.cni_version.clone(); // for error
			info!("ipam-consul serving spec v{} command=Del", cni_version);

			let res: AppResult<IpamSuccessReply> = block_on(async move {
				let ipam = config.ipam.clone().ok_or(CniError::MissingField("ipam"))?;
				debug!("ipam={:?}", ipam);

				let prev_result: Option<IpamSuccessReply> = config
					.prev_result
					.map(|p| serde_json::from_value(p).map_err(CniError::Json))
					.transpose()?;
				debug!("prevResult={:?}", prev_result);

				Ok(IpamSuccessReply {
					cni_version: config.cni_version,
					ips: Vec::new(),
					routes: Vec::new(),
					dns: Default::default(),
					specific: Default::default(),
				})
			});

			match res {
				Ok(res) => reply(res),
				Err(res) => {
					error!("error: {}", res);
					reply(res.into_result(cni_version))
				}
			}
		}
		Cni::Check {
			container_id,
			config,
			..
		} => {}
		Cni::Version(_) => unreachable!(),
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PoolEntry {
	pub target: String,
}

#[derive(Clone, Debug, Deserialize)]
struct Pool {
	name: String,
	requested_ip: Option<IpAddr>,
}
