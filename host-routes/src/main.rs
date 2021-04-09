use std::{
	net::IpAddr,
	time::{Duration, Instant},
};

use async_std::{
	future::timeout,
	task::{block_on, spawn_blocking},
};
use cni_plugin::{
	error::CniError,
	reply::{reply, SuccessReply},
	Cni, Command, Inputs,
};
use ipnetwork::IpNetwork;
use log::{debug, error, info};
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

		let routing: Vec<Routing> = serde_json::from_str(&eval)?;
		info!("got {} routings from jq expression", routing.len());

		let mut errors = Vec::with_capacity(routing.len());
		let mut applied = Vec::with_capacity(routing.len());
		for route in routing {
			if let Err(err) = if matches!(command, Command::Del) {
				debug!("deleting {:?}", route);
				route.del()
			} else {
				debug!("adding {:?}", route);
				route.add()
			} {
				errors.push(err);
			} else {
				info!("applied route to {}", route.prefix);
				applied.push(serde_json::to_value(route)?);
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

			let existing_routes = reply
				.specific
				.entry("hostRoutes".into())
				.or_insert_with(|| Value::Array(Vec::new()));

			if let Some(r) = existing_routes.as_array_mut() {
				debug!("existing host routes: {:?}", r);
				info!("returning {} applied routes", applied.len());
				r.extend(applied);
			} else {
				return Err(CniError::InvalidField {
					field: "prevResult.hostRoutes",
					expected: "array",
					value: existing_routes.clone(),
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
struct Routing {
	pub prefix: IpNetwork,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub device: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gateway: Option<IpAddr>,
}

impl Routing {
	pub fn add(&self) -> Result<(), CniError> {
		// TODO: add route, replacing existing entry if present
		Ok(())
	}

	pub fn del(&self) -> Result<(), CniError> {
		// TODO: delete route
		Ok(())
	}
}
