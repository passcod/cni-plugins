use std::{collections::BTreeMap, net::{IpAddr, Ipv4Addr, Ipv6Addr}, str::FromStr};

use async_std::task::block_on;
use cni_plugin::{
	error::CniError,
	ip_range::IpRange,
	reply::{reply, IpReply, IpamSuccessReply},
	config::Route,
	Cni, Command, Inputs,
};
use consul::ConsulValue;
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::consul::ConsulPair;
use crate::error::{AppError, AppResult};

mod consul;
mod error;

fn main() {
	cni_plugin::install_logger("ipam-da-consul.log");

	// UNWRAP: None on Version, but Version is handled by load()
	let Inputs {
		command,
		container_id,
		config,
		..
	} = Cni::load().into_inputs().unwrap();

	let cni_version = config.cni_version.clone(); // for error
	info!(
		"ipam-da-consul serving spec v{} for command={:?}",
		cni_version, command
	);

	let res: AppResult<IpamSuccessReply> = block_on(async move {
		let ipam = config.ipam.clone().ok_or(CniError::MissingField("ipam"))?;
		debug!("ipam={:?}", ipam);

		let prev_result: Option<IpamSuccessReply> = config
			.prev_result
			.map(|p| serde_json::from_value(p).map_err(CniError::Json))
			.transpose()?;
		debug!("prevResult={:?}", prev_result);

		let pools: Vec<Pool> = prev_result
			.as_ref()
			.map(|p| p.specific.get("pools").cloned())
			.flatten()
			.map(|p| serde_json::from_value(p).map_err(CniError::Json))
			.transpose()?
			.unwrap_or_default();
		debug!("pools={:?}", pools);
		// TODO: support multiple?

		let consul_servers = ipam
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

		let consul_url = good_server(&consul_servers).await?;

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

		match command {
			Command::Add => {
				let pool = pool_def(&consul_url, &pool_name).await?;

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
					let pool_known = pool_known(&consul_url, &pool_name).await?;

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
					consul_url.join(&format!("v1/kv/ipam/{}/{}", pool_name, ip.ip()))?;

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
					.await?;

				if success {
					info!("allocated address {}", ip);
					Ok(IpamSuccessReply {
						cni_version: config.cni_version,
						routes: vec![
							Route {
								dst: match ip {
									IpNetwork::V4(_) => IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(0,0,0,0),0).unwrap()),
									IpNetwork::V6(_) => IpNetwork::V6(Ipv6Network::new(Ipv6Addr::new(0,0,0,0,0,0,0,0),0).unwrap()),
								},
								gw: gateway.clone(),
							}
						],
						ips: vec![IpReply {
							address: ip,
							gateway,
							interface: None,
						}],
						dns: Default::default(),
						specific: Default::default(),
					})
				} else {
					error!("consul write to ipam/{}/{} returned false", pool_name, ip);
					Err(AppError::ConsulWriteFailed)
				}
			}
			Command::Del => {
				debug!(
					"finding all known IPs in pool={} with target={}",
					pool_name, container_id
				);
				let pool_known = pool_known(&consul_url, &pool_name).await?;
				let rip = pool_known.into_iter().filter_map(|(ip, entry)| {
					if entry.target == container_id {
						Some((format!("ipam/{}/{}", pool_name, ip), entry.index))
					} else {
						None
					}
				});

				// TODO: do we actually want a transaction? should we best-effort delete instead?
				consul::delete_all(&consul_url, rip).await?;

				Ok(IpamSuccessReply {
					cni_version: config.cni_version,
					ips: Vec::new(),
					routes: Vec::new(),
					dns: Default::default(),
					specific: Default::default(),
				})
			}
			Command::Check => {
				todo!()
			}
			Command::Version => unreachable!(),
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

async fn good_server<'u>(list: &'u [Url]) -> AppResult<&'u Url> {
	let mut last_err = None;
	for url in list {
		match surf::get(url.join("v1/kv/ipam/")?).await {
			Ok(res) if res.status().is_success() => {
				debug!("found good consul server: {}", url);
				return Ok(url);
			}
			Ok(res) => {
				warn!("bad consul server, trying next. status={}", res.status());
				last_err = Some(
					CniError::Generic(format!("error status from consul: {}", res.status())).into(),
				);
			}
			Err(err) => {
				warn!("unreachable consul server, trying next. err={}", err);
				last_err = Some(err.into());
			}
		}
	}

	if let Some(err) = last_err {
		Err(err)
	} else {
		// LEAK: is on error path so we exit nearly immediately anyway
		Err(AppError::from(CniError::InvalidField {
			field: "consul_servers",
			expected: "list of servers",
			value: serde_json::to_value(list).map_err(CniError::Json)?,
		}))
	}
}

async fn pool_def(consul_url: &Url, name: &str) -> AppResult<Vec<IpRange>> {
	let pool_url = consul_url.join(&format!("v1/kv/ipam/{}", name))?;
	let pool: Vec<ConsulPair<Vec<IpRange>>> = surf::get(pool_url).recv_json().await?;

	let pool = pool
		.into_iter()
		.next()
		.ok_or(AppError::MissingResource {
			remote: "consul",
			resource: "pool",
			path: format!("ipam/{}", name),
		})?
		.parsed_value()
		.map_err(|err| AppError::InvalidResource {
			remote: "consul",
			resource: "pool",
			path: format!("ipam/{}", name),
			err: Box::new(err),
		})?
		.ok_or(AppError::InvalidResource {
			remote: "consul",
			resource: "pool",
			path: format!("ipam/{}", name),
			err: Box::new(CniError::Generic(
				"expected IpRange as JSON, got null".into(),
			)),
		})?;

	debug!("pool={:?}", pool);
	Ok(pool)
}

async fn pool_known(consul_url: &Url, name: &str) -> AppResult<BTreeMap<IpAddr, KnownPoolEntry>> {
	let mut url = consul_url.join(&format!("v1/kv/ipam/{}/", name))?;
	url.set_query(Some("recurse"));
	let known: Vec<ConsulPair<PoolEntry>> = surf::get(url).recv_json().await?;
	let known: BTreeMap<IpAddr, KnownPoolEntry> =
		known
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
						let index = pair.modify_index;
						pair.key
							.split('/')
							.last()
							.ok_or_else(|| {
								unreachable!("due to how the key is constructed it will always have at least one segment")
							})
							.and_then(|ip| {
								IpAddr::from_str(ip).map_err(|err| AppError::InvalidResource {
									remote: "consul",
									resource: "ip-pool",
									path: key.clone(),
									err: Box::new(CniError::Generic(format!(
										"expected key to be an IP address; {}",
										err
									))),
								})
							})
							.map(|ip| {
								(
									ip,
									if let ConsulValue::Parsed(v) = pair.value {
										KnownPoolEntry {
											target: v.target,
											index,
										}
									} else {
										unreachable!("consul value should be parsed and nulls already filtered")
									},
								)
							})
					})
			})
			.collect::<AppResult<BTreeMap<_, _>>>()?;

	debug!("pool-known={:?}", known);
	Ok(known)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PoolEntry {
	pub target: String,
}

#[derive(Clone, Debug)]
struct KnownPoolEntry {
	pub target: String,
	pub index: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct Pool {
	name: String,
	requested_ip: Option<IpAddr>,
}
