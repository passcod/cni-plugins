use async_std::task::block_on;
use cni_plugin::{
	error::CniError,
	reply::{reply, SuccessReply},
	Cni, Command,
};
use log::{debug, error, info};

fn main() {
	cni_plugin::install_logger("advertise.log");
	debug!(
		"{} (CNI post plugin) version {}",
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
		"advertise serving spec v{} for command={:?}",
		cni_version, command
	);

	let res: Result<SuccessReply, CniError> = block_on(async move {
		debug!("config={:#?}", config);

		match command {
			Command::Add => std::process::exit(0), //Err(CniError::Generic("TODO".into())),
			Command::Del => Ok(SuccessReply {
				cni_version: config.cni_version,
				interfaces: Default::default(),
				ips: Default::default(),
				routes: Default::default(),
				dns: Default::default(),
				specific: Default::default(),
			}),
			Command::Check => Err(CniError::Generic("TODO".into())),
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
