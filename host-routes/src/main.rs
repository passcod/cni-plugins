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
	reply::{reply, SuccessReply},
	Cni, Command, Inputs,
};
use futures::{
	stream::{FuturesOrdered, TryStreamExt},
	StreamExt,
};
use ipnetwork::IpNetwork;
use log::{debug, error, info, warn};
use rtnetlink::{Handle, IpVersion, LinkHandle, RouteHandle};
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
			.get("routing")
			.ok_or(CniError::MissingField("routing"))
			.and_then(|val| {
				val.as_str().ok_or_else(|| CniError::InvalidField {
					field: "routing",
					expected: "string",
					value: val.clone(),
				})
			})?
			.to_owned();
		debug!("routing={:?}", expr);

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

		let routing: Vec<Routing> = serde_json::from_str(&eval)?;
		info!("got {} routings from jq expression", routing.len());
		let trials: Vec<_> = routing
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

		let existing_routes = reply
			.specific
			.entry("hostRoutes".into())
			.or_insert_with(|| Value::Array(Vec::new()));

		if let Some(r) = existing_routes.as_array_mut() {
			debug!("existing host routes: {:?}", r);
			info!("returning {} applied routes", outcomes.len());
			r.extend(
				outcomes
					.into_iter()
					.map(|o| serde_json::to_value(o.route))
					.collect::<Result<Vec<Value>, _>>()?,
			);
		} else {
			return Err(CniError::InvalidField {
				field: "prevResult.hostRoutes",
				expected: "array",
				value: existing_routes.clone(),
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
	pub route: Routing,
	pub tries: u8,
	pub link: Option<Option<u32>>,
	pub last_error: Option<CniError>,
}

impl Trial {
	pub fn new(
		route: Routing,
		netlink: Handle,
		command: Command,
		tries: u8,
	) -> Result<Self, CniError> {
		Ok(Self {
			netlink,
			command,
			route: route.validate()?,
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
					self.route, nap
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
		let mut nlrh = RouteHandle::new(self.netlink.clone());

		let link = if let Some(link) = self.link {
			link
		} else {
			let link = self.route.link_index(&mut nllh).await?;
			self.link = Some(link);
			link
		};

		if matches!(self.command, Command::Del) {
			debug!("deleting {:?}", self.route);
			self.route.del(&mut nlrh, link).await?;
			info!("deleted route to {}", self.route.prefix);
		} else {
			debug!("adding {:?}", self.route);
			self.route.add(&mut nlrh, link).await?;
			info!("added route to {}", self.route.prefix);
		}

		Ok(())
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Routing {
	pub prefix: IpNetwork,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub device: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gateway: Option<IpAddr>,
}

impl Routing {
	pub fn validate(self) -> Result<Self, CniError> {
		if self.device.is_none() && self.gateway.is_none() {
			Err(CniError::Generic(
				"at least one of device or gateway is required, none provided".into(),
			))
		} else {
			Ok(self)
		}
	}

	pub async fn add(&self, nlrh: &mut RouteHandle, link: Option<u32>) -> Result<(), CniError> {
		debug!("first, attempting to delete route {:?}", self);
		if let Err(err) = self.del(nlrh, link).await {
			warn!("pre-emptive delete of route {:?} failed: {}", self, err);
		}

		debug!("making route add");
		let mut add = nlrh.add();
		if let Some(index) = link {
			debug!("route add: with output interface {}", index);
			add = add.output_interface(index);
		}

		match self.prefix {
			IpNetwork::V4(net) => {
				debug!("route add: with v4 prefix: {}", net);
				let mut add = add.v4().destination_prefix(net.ip(), net.prefix());

				if let Some(IpAddr::V4(gw)) = self.gateway {
					debug!("route add: with gateway {}", gw);
					add = add.gateway(gw);
				}

				debug!("route add: execute");
				add.execute().await.map_err(nlerror)?;
				debug!("route add: done");
			}
			IpNetwork::V6(net) => {
				debug!("route add: with v6 prefix: {}", net);
				let mut add = add.v6().destination_prefix(net.ip(), net.prefix());

				if let Some(IpAddr::V6(gw)) = self.gateway {
					debug!("route add: with gateway {}", gw);
					add = add.gateway(gw);
				}

				debug!("route add: execute");
				add.execute().await.map_err(nlerror)?;
				debug!("route add: done");
			}
		}

		Ok(())
	}

	pub async fn del(&self, nlrh: &mut RouteHandle, link: Option<u32>) -> Result<(), CniError> {
		let ipv = match self.prefix {
			IpNetwork::V4(_) => IpVersion::V4,
			IpNetwork::V6(_) => IpVersion::V6,
		};

		debug!("getting all {:?} routes", ipv);
		let mut routes = nlrh.get(ipv).execute();

		debug!("iterating routes");
		let mut n = 0;
		while let Some(route) = routes.try_next().await.map_err(nlerror)? {
			n += 1;

			debug!(
				"route {}: link index={:?}, query={:?}",
				n,
				route.output_interface(),
				link
			);
			if route.output_interface() != link {
				continue;
			}

			debug!(
				"route {}: prefix={:?}, query={:?}",
				n,
				route.destination_prefix(),
				self.prefix
			);
			if route.destination_prefix() != Some((self.prefix.ip(), self.prefix.prefix())) {
				continue;
			}

			debug!(
				"route {}: gateway={:?}, query={:?}",
				n,
				route.gateway(),
				self.gateway
			);
			if route.gateway() != self.gateway {
				continue;
			}

			info!("deleting found route\n  input interface: {:?}\n  output interface: {:?}\n  source prefix: {:?}\n  dest prefix: {:?}\n  gateway: {:?}", route.input_interface(), route.output_interface(), route.source_prefix(), route.destination_prefix(), route.gateway());
			nlrh.del(route).execute().await.map_err(nlerror)?;
		}

		debug!("iterated {} routes", n);
		Ok(())
	}

	pub async fn link_index(&self, nllh: &mut LinkHandle) -> Result<Option<u32>, CniError> {
		if let Some(ref dev) = self.device {
			let mut linklist = nllh.get().set_name_filter(dev.clone()).execute();
			if let Some(link) = linklist.try_next().await.map_err(nlerror)? {
				info!("link: {:?}", link.header);
				Ok(Some(link.header.index))
			} else {
				Err(CniError::Generic(format!(
					"interface not found for route {:?}",
					self
				)))
			}
		} else {
			Ok(None)
		}
	}
}

fn nlerror(err: rtnetlink::Error) -> CniError {
	CniError::Generic(format!("netlink: {}", err))
}
