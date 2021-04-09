use std::collections::HashMap;

use async_std::task::block_on;
use cni_plugin::{
	error::CniError,
	reply::{reply, IpamSuccessReply},
	Cni,
};
use log::{debug, error, info};

fn main() {
	cni_plugin::logger::install(env!("CARGO_PKG_NAME"));
	debug!(
		"{} (CNI IPAM delegate plugin) version {}",
		env!("CARGO_PKG_NAME"),
		env!("CARGO_PKG_VERSION")
	);

	match Cni::load() {
		Cni::Add { config, .. } | Cni::Del { config, .. } | Cni::Check { config, .. } => {
			let cni_version = config.cni_version.clone(); // for error
			info!(
				"{} serving spec v{} for command=any",
				env!("CARGO_PKG_NAME"),
				cni_version
			);

			let res: Result<IpamSuccessReply, CniError> = block_on(async move {
				let ipam = config.ipam.clone().ok_or(CniError::MissingField("ipam"))?;
				debug!("ipam={:?}", ipam);

				let pools = ipam
					.specific
					.get("pools")
					.ok_or(CniError::MissingField("ipam.pools"))?
					.clone();

				let mut specific = HashMap::new();
				specific.insert("pools".into(), pools);

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
					dns: Default::default(),
					specific,
				})
			});

			match res {
				Ok(res) => reply(res),
				Err(res) => {
					error!("error: {}", res);
					reply(res.into_reply(cni_version))
				}
			}
		}
		Cni::Version(_) => unreachable!(),
	}
}
