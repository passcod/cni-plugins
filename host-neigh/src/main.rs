use std::{
	convert::TryFrom,
	net::IpAddr,
	time::{Duration, Instant},
};

use async_std::{
	future::timeout,
	task::{block_on, sleep, spawn, spawn_blocking},
};
use cni_plugin::{
	error::CniError,
	logger,
	macaddr::MacAddr,
	reply::{reply, SuccessReply},
	Cni, Command, Inputs,
};
use futures::{
	stream::{FuturesOrdered, TryStreamExt},
	StreamExt,
};
use log::{debug, error, info, warn};
use rtnetlink::{
	packet::rtnl::neighbour::nlas::Nla, Handle, IpVersion, LinkHandle, NeighbourHandle,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn main() {
	let mut logconfig = logger::default_config();
	logconfig.add_filter_ignore_str("netlink_proto");
	logger::with_config(env!("CARGO_PKG_NAME"), logconfig.build());

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

		let tries = config
			.specific
			.get("neigh")
			.and_then(|val| val.as_u64())
			.and_then(|n| u8::try_from(n).ok())
			.map(|n| if n == 0 || n > 10 { 10 } else { n })
			.unwrap_or(3);

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

		debug!("initialising netlink");
		let (nlconn, nl, _) = rtnetlink::new_connection()?;

		let neighs: Vec<Neigh> = serde_json::from_str(&eval)?;
		info!("got {} neighs from jq expression", neighs.len());
		let trials: Vec<Trial> = neighs
			.into_iter()
			.map(|n| Trial::new(n, nl.clone(), command, tries))
			.collect::<Result<_, _>>()?;

		debug!("starting netlink connection task");
		spawn(nlconn);

		let mut outcomes = trials
			.into_iter()
			.map(Trial::run)
			.collect::<FuturesOrdered<_>>()
			.collect::<Vec<Trial>>()
			.await;

		let error = outcomes
			.iter_mut()
			.filter_map(|t| t.last_error.take().map(|e| e.to_string()))
			.collect::<Vec<String>>()
			.join("\n");
		if !error.is_empty() {
			return Err(CniError::Generic(error));
		}

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
			info!("returning {} applied neighs", outcomes.len());
			r.extend(
				outcomes
					.into_iter()
					.map(|o| serde_json::to_value(o.neigh))
					.collect::<Result<Vec<Value>, _>>()?,
			);
		} else {
			return Err(CniError::InvalidField {
				field: "prevResult.hostNeighbours",
				expected: "array",
				value: existing_neighs.clone(),
			});
		}

		Ok(reply)
	});

	match res {
		Ok(res) => reply(res),
		Err(res) => {
			error!("error: {}", res);
			reply(res.into_reply(cni_version))
		}
	}
}

#[derive(Debug)]
struct Trial {
	pub netlink: Handle,
	pub command: Command,
	pub neigh: Neigh,
	pub tries: u8,
	pub link: Option<u32>,
	pub last_error: Option<CniError>,
}

impl Trial {
	pub fn new(
		neigh: Neigh,
		netlink: Handle,
		command: Command,
		tries: u8,
	) -> Result<Self, CniError> {
		Ok(Self {
			netlink,
			command,
			neigh: neigh.validate(command)?,
			tries,
			link: None,
			last_error: None,
		})
	}

	pub async fn run(mut self) -> Self {
		for _ in 0..self.tries {
			if let Err(err) = self.try_once().await {
				self.last_error = Some(err);

				let nap = Duration::from_millis(50);
				warn!(
					"got an error applying {:?}, waiting {:?} before next try",
					self.neigh, nap
				);
				sleep(nap).await;
			} else {
				break;
			}
		}

		self
	}

	async fn try_once(&mut self) -> Result<(), CniError> {
		let mut nllh = LinkHandle::new(self.netlink.clone());
		let mut nlnh = NeighbourHandle::new(self.netlink.clone());

		let link = if let Some(link) = self.link {
			link
		} else {
			let link = self.neigh.link_index(&mut nllh).await?;
			self.link = Some(link);
			link
		};

		if matches!(self.command, Command::Del) {
			debug!("deleting {:?}", self.neigh);
			self.neigh.del(&mut nlnh, link).await?;
			info!("deleted {} neighbour from {}", self.neigh.address, link);
		} else {
			debug!("adding {:?}", self.neigh);
			self.neigh.add(&mut nlnh, link).await?;
			info!("added {} neighbour to {}", self.neigh.address, link);
		}

		Ok(())
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Neigh {
	pub address: IpAddr,
	pub device: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub lladdr: Option<MacAddr>,
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
