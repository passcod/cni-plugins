use async_std::task::block_on;
use cni_plugin::{
	delegate,
	error::CniError,
	reply::{reply, IpamSuccessReply},
	Cni, Command,
};
use serde_json::Value;

fn main() {
	match Cni::load() {
		Cni::Add {
			container_id,
			mut config,
			..
		} => {
			let cni_version = config.cni_version.clone(); // for error
			let res: Result<IpamSuccessReply, CniError> = block_on(async move {
				let ipam = config.ipam.clone().ok_or(CniError::MissingField("ipam"))?;

				let get_config = |name: &'static str| -> Result<&Value, CniError> {
					ipam.specific
						.get(name)
						.ok_or(CniError::MissingField("ipam"))
				};

				let config_string = |name: &'static str| -> Result<String, CniError> {
					get_config(name).and_then(|v| {
						if let Value::String(s) = v {
							Ok(s.to_owned())
						} else {
							Err(CniError::InvalidField {
								field: name,
								expected: "string",
								value: v.clone(),
							})
						}
					})
				};

				let selection_plugin = config_string("selection_plugin")?;
				let allocation_plugin = config_string("allocation_plugin")?;

				let selection_result: IpamSuccessReply =
					delegate(&selection_plugin, Command::Add, &config).await?;
				config.prev_result = Some(serde_json::to_value(&selection_result)?);
				let allocation_result: IpamSuccessReply =
					delegate(&allocation_plugin, Command::Add, &config).await?;

				// return ipam result

				Err(CniError::Debug(Box::new((
					selection_plugin,
					selection_result,
					allocation_plugin,
					allocation_result,
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
