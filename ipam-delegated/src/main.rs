use async_std::task::block_on;
use cni_plugin::{
	delegate,
	error::CniError,
	reply::{reply, IpamSuccessReply},
	Cni, Command,
};
use log::{debug, error, info};
use serde_json::{from_value, to_value};

fn main() {
	cni_plugin::install_logger("ipam-delegated.log");
	debug!(
		"{} (CNI IPAM plugin) version {}",
		env!("CARGO_PKG_NAME"),
		env!("CARGO_PKG_VERSION")
	);

	let cni = Cni::load();

	let (command, config) = match cni {
		Cni::Add { config, .. } => (Command::Add, config),
		Cni::Del { config, .. } => (Command::Del, config),
		Cni::Check { config, .. } => (Command::Check, config),
		Cni::Version(_) => unreachable!(),
	};
	let cni_version = config.cni_version.clone(); // for error
	info!(
		"ipam-delegated serving spec v{} for command={:?}",
		cni_version, command
	);

	let res: Result<IpamSuccessReply, CniError> = block_on(async move {
		let delegated_plugins = config
			.ipam
			.clone()
			.ok_or(CniError::MissingField("ipam"))?
			.specific
			.get("delegates")
			.ok_or(CniError::MissingField("ipam.delegates"))
			.and_then(|v| {
				let v: Vec<String> = from_value(v.to_owned())?;
				Ok(v)
			})?;

		debug!("delegated plugin list: {:?}", delegated_plugins);
		if delegated_plugins.is_empty() {
			return Err(CniError::InvalidField {
				field: "ipam.delegates",
				expected: "at least one plugin",
				value: Vec::<()>::new().into(),
			});
		}

		let mut config = config;
		match command {
			Command::Add => {
				let mut last_result = None;
				let mut undo: Vec<String> = Vec::with_capacity(delegated_plugins.len());

				for plugin in delegated_plugins {
					undo.push(plugin.clone());

					let result: IpamSuccessReply =
						match delegate(&plugin, Command::Add, &config).await {
							Ok(reply) => reply,
							Err(err) => {
								let mut errors = Vec::with_capacity(undo.len() + 1);
								errors.push((plugin.clone(), err));

								for plugin in undo {
									let result: IpamSuccessReply =
										match delegate(&plugin, Command::Del, &config).await {
											Ok(reply) => reply,
											Err(err) => {
												errors.push((plugin, err));
												continue;
											}
										};

									config.prev_result = Some(to_value(&result)?);
								}

								return Err(multi_error(errors));
							}
						};

					config.prev_result = Some(to_value(&result)?);
					last_result = Some(result);
				}

				if let Some(result) = last_result {
					Ok(result)
				} else {
					Err(CniError::Generic("no IPAM delegated plugins ran".into()))
				}
			}
			Command::Del | Command::Check => {
				let mut last_result = None;
				let mut errors = Vec::with_capacity(delegated_plugins.len());

				for plugin in delegated_plugins {
					let result: IpamSuccessReply = match delegate(&plugin, command, &config).await {
						Ok(reply) => reply,
						Err(err) => {
							errors.push((plugin, err));
							continue;
						}
					};

					config.prev_result = Some(to_value(&result)?);
					last_result = Some(result);
				}

				if !errors.is_empty() {
					Err(multi_error(errors))
				} else if let Some(result) = last_result {
					Ok(result)
				} else {
					Err(CniError::Generic("no IPAM delegated plugins ran".into()))
				}
			}
			Command::Version => unreachable!(),
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

fn multi_error(errors: Vec<(String, CniError)>) -> CniError {
	CniError::Delegated {
		err: Box::new(CniError::Generic(
			errors
				.iter()
				.map(|e| e.1.to_string())
				.collect::<Vec<String>>()
				.join("\n"),
		)),
		plugin: errors
			.into_iter()
			.map(|e| e.0)
			.collect::<Vec<String>>()
			.join(","),
	}
}
