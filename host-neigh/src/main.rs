use std::{
	net::IpAddr,
	time::{Duration, Instant},
};

use async_std::{
	future::timeout,
	task::{block_on, spawn, spawn_blocking},
};
use cni_plugin::{
	error::CniError,
	macaddr::MacAddr,
	reply::{reply, SuccessReply},
	Cni, Command, Inputs,
};
use futures::stream::TryStreamExt;
use log::{debug, error, info, warn};
use rtnetlink::{packet::rtnl::neighbour::nlas::Nla, IpVersion, LinkHandle, NeighbourHandle};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn main() {
	cni_plugin::install_logger(env!("CARGO_PKG_NAME"));
	debug!(
		"{} (CNI post plugin) version {}",
		env!("CARGO_PKG_NAME"),
		env!("CARGO_PKG_VERSION")
	);

	// UNWRAP: unreachable due to using load()
	let Inputs {
		command, config, ..
	} = Cni::load().into_inputs().unwrap();

	let cni_version = config.cni_version.clone(); // for error
	info!(
		"{} serving spec v{} for command={:?}",
		env!("CARGO_PKG_NAME"),
		cni_version,
		command
	);

	let res: Result<SuccessReply, CniError> = block_on(async move {
		if matches!(command, Command::Check) {
			return Err(CniError::Generic("TODO".into()));
		}

		let expr = config
			.specific
			.get("neigh")
			.ok_or(CniError::MissingField("neigh"))
			.and_then(|val| {
				val.as_str().ok_or_else(|| CniError::InvalidField {
					field: "neigh",
					expected: "string",
					value: val.clone(),
				})
			})?
			.to_owned();
		debug!("neigh={:?}", expr);

		let input = serde_json::to_string(&config)?;

		debug!("spawning jq");
		let pre = Instant::now();
		let eval: String = timeout(
			Duration::from_secs(1),
			spawn_blocking(move || jq_rs::run(&expr, &input).map_err(|err| err.to_string())),
		)
		.await
		.map_err(|err| CniError::Generic(format!("jq evaluation timed out: {}", err)))?
		.map_err(CniError::Generic)?;

		info!("ran jq expression in {:?}", pre.elapsed());
		debug!("jq eval={:?}", eval);

		let neighs: Vec<Neigh> = serde_json::from_str(&eval)?;
		info!("got {} neighs from jq expression", neighs.len());
		let neighs: Vec<_> = neighs
			.into_iter()
			.map(|n| Neigh::validate(n, command))
			.collect::<Result<_, _>>()?;

		debug!("connecting to netlink");
		let (nlconn, nl, _) = rtnetlink::new_connection()?;
		spawn(nlconn);
		let mut nllh = LinkHandle::new(nl.clone());
		let mut nlnh = NeighbourHandle::new(nl);

		let mut errors = Vec::with_capacity(neighs.len());
		let mut applied = Vec::with_capacity(neighs.len());

		// TODO: apply neighbours in parallel
		for neigh in neighs {
			let link = neigh.link_index(&mut nllh).await?;

			if let Err(err) = if matches!(command, Command::Del) {
				debug!("deleting {:?}", neigh);
				neigh.del(&mut nlnh, link).await
			} else {
				debug!("adding {:?}", neigh);
				neigh.add(&mut nlnh, link).await
			} {
				if matches!(command, Command::Add) && neigh.critical {
					errors.push(err);
				} else {
					warn!("non-critical neigh {:?} failed: {}", neigh, err);
				}
			} else {
				info!("applied {} neighbour on {}", neigh.address, link);
				applied.push(serde_json::to_value(neigh)?);
			}
		}

		if errors.is_empty() {
			let cni_version = config.cni_version.clone();
			let mut reply = config
				.prev_result
				.map(|val| serde_json::from_value(val).map_err(CniError::Json))
				.transpose()?
				.unwrap_or_else(|| SuccessReply {
					cni_version,
					interfaces: Default::default(),
					ips: Default::default(),
					routes: Default::default(),
					dns: Default::default(),
					specific: Default::default(),
				});

			let existing_neighs = reply
				.specific
				.entry("hostNeighbours".into())
				.or_insert_with(|| Value::Array(Vec::new()));

			if let Some(r) = existing_neighs.as_array_mut() {
				debug!("existing host neighbours: {:?}", r);
				info!("returning {} applied neighs", applied.len());
				r.extend(applied);
			} else {
				return Err(CniError::InvalidField {
					field: "prevResult.hostNeighbours",
					expected: "array",
					value: existing_neighs.clone(),
				});
			}

			Ok(reply)
		} else {
			Err(CniError::Generic(
				errors
					.iter()
					.map(|e| e.to_string())
					.collect::<Vec<String>>()
					.join("\n"),
			))
		}
	});

	match res {
		Ok(res) => reply(res),
		Err(res) => {
			error!("error: {}", res);
			reply(res.into_reply(cni_version))
		}
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Neigh {
	pub address: IpAddr,
	pub device: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub lladdr: Option<MacAddr>,
	#[serde(default = "critdef", skip_serializing)]
	pub critical: bool,
}

fn critdef() -> bool {
	true
}

impl Neigh {
	pub fn validate(self, command: Command) -> Result<Self, CniError> {
		if self.lladdr.is_none() && !matches!(command, Command::Del) {
			Err(CniError::Generic(
				"lladdr is required when command is not DEL".into(),
			))
		} else {
			Ok(self)
		}
	}

	pub async fn add(&self, nlnh: &mut NeighbourHandle, link: u32) -> Result<(), CniError> {
		debug!("first, attempting to delete neighbour {:?}", self);
		if let Err(err) = self.del(nlnh, link).await {
			warn!("pre-emptive delete of neighbour {:?} failed: {}", self, err);
		}

		// UNWRAP: validated for add command
		let lladdr = self.lladdr.unwrap();
		let lladdr = lladdr.0.as_bytes();

		debug!("adding neighbour {:?}", self);
		nlnh.add(link, self.address)
			.link_local_address(lladdr)
			.execute()
			.await
			.map_err(nlerror)?;
		debug!("added neighbour {:?}", self);

		Ok(())
	}

	pub async fn del(&self, nlnh: &mut NeighbourHandle, link: u32) -> Result<(), CniError> {
		let ipv = match self.address {
			IpAddr::V4(_) => IpVersion::V4,
			IpAddr::V6(_) => IpVersion::V6,
		};

		debug!("getting all {:?} neighbours", ipv);
		let mut neighs = nlnh.get().set_family(ipv).execute();

		debug!("iterating neighbours");
		let mut n = 0;
		while let Some(neigh) = neighs.try_next().await.map_err(nlerror)? {
			n += 1;

			debug!(
				"neigh {}: link index={}, query={}",
				n, neigh.header.ifindex, link
			);
			if neigh.header.ifindex != link {
				continue;
			}

			if let Some(lladdr) = self.lladdr {
				let ll = match neigh
					.nlas
					.iter()
					.filter_map(|n| {
						if let Nla::LinkLocalAddress(d) = n {
							Some(d)
						} else {
							None
						}
					})
					.next()
				{
					Some(l) => l,
					None => continue,
				};

				debug!("neigh {}: lladdr={:?}, query={}", n, ll, lladdr);
				if lladdr.0.as_bytes() != ll {
					continue;
				}
			}

			let dest = match neigh
				.nlas
				.iter()
				.filter_map(|n| {
					if let Nla::Destination(d) = n {
						Some(d)
					} else {
						None
					}
				})
				.next()
			{
				Some(d) => d,
				None => continue,
			};

			debug!("neigh {}: address={:?}, query={}", n, dest, self.address);
			match self.address {
				IpAddr::V4(v4) => {
					if &v4.octets()[..] != dest {
						continue;
					}
				}
				IpAddr::V6(v6) => {
					if &v6.octets()[..] != dest {
						continue;
					}
				}
			}

			info!("deleting found neighbour {:?}", neigh);
			nlnh.del(neigh).execute().await.map_err(nlerror)?;
		}

		debug!("iterated {} neighbours", n);
		Ok(())
	}

	pub async fn link_index(&self, nllh: &mut LinkHandle) -> Result<u32, CniError> {
		let mut linklist = nllh.get().set_name_filter(self.device.clone()).execute();
		if let Some(link) = linklist.try_next().await.map_err(nlerror)? {
			info!("link: {:?}", link.header);
			Ok(link.header.index)
		} else {
			Err(CniError::Generic(format!(
				"interface not found for route {:?}",
				self
			)))
		}
	}
}

fn nlerror(err: rtnetlink::Error) -> CniError {
	CniError::Generic(format!("netlink: {}", err))
}
