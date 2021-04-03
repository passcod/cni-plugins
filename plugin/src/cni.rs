use std::{
	env,
	io::{stdin, Read},
	path::PathBuf,
	str::FromStr,
};

use log::{debug, error};
use regex::Regex;
use semver::Version;

use crate::{command::Command, config::NetworkConfig, error::{
	CniError,
	EmptyValueError,
	RegexValueError,
}, path::CniPath, reply::reply, version::VersionPayload};

#[derive(Clone, Debug)]
pub enum Cni {
	Add {
		container_id: String,
		ifname: String,
		netns: PathBuf,
		path: Vec<PathBuf>,
		config: NetworkConfig,
	},
	Del {
		container_id: String,
		ifname: String,
		netns: Option<PathBuf>,
		path: Vec<PathBuf>,
		config: NetworkConfig,
	},
	Check {
		container_id: String,
		ifname: String,
		netns: PathBuf,
		path: Vec<PathBuf>,
		config: NetworkConfig,
	},
	Version(Version),
}

impl Cni {
	// TODO: in doc: CNI_ARGS is deprecated in the spec, and we deliberately
	// chose to ignore it here.
	pub fn from_env() -> Result<Self, CniError> {
		fn require_env<T>(var: &'static str) -> Result<T, CniError>
		where
			T: FromStr,
			T::Err: std::error::Error + 'static,
		{
			env::var(var)
				.map_err(|err| CniError::MissingEnv { var, err })
				.and_then(|val| {
					debug!("read env var {} = {:?}", var, val);
					val.parse().map_err(|err| CniError::InvalidEnv {
						var,
						err: Box::new(err),
					})
				})
		}
		fn load_env<T>(var: &'static str) -> Result<Option<T>, CniError>
		where
			T: FromStr,
			T::Err: std::error::Error + 'static,
		{
			require_env(var).map(Some).or_else(|err| {
				if let CniError::MissingEnv { .. } = err {
					Ok(None)
				} else {
					Err(err)
				}
			})
		}

		let path: CniPath = load_env("CNI_PATH")?.unwrap_or_default();
		let path = path.0;

		let mut payload = Vec::with_capacity(1024);
		debug!("reading stdin til EOF...");
		stdin().read_to_end(&mut payload)?;
		debug!("read payload bytes={}", payload.len());
		if payload.is_empty() {
			return Err(CniError::MissingInput);
		}

		fn check_container_id(id: &str) -> Result<(), CniError> {
			if id.is_empty() {
				return Err(CniError::InvalidEnv {
					var: "CNI_CONTAINERID",
					err: Box::new(EmptyValueError),
				});
			}

			let re = Regex::new(r"^[a-z0-9][a-z0-9_.\-]*$").unwrap();
			if !re.is_match(id) {
				return Err(CniError::InvalidEnv {
					var: "CNI_CONTAINERID",
					err: Box::new(RegexValueError(re)),
				});
			}

			Ok(())
		}

		match require_env("CNI_COMMAND")? {
			Command::Add => {
				let container_id: String = require_env("CNI_CONTAINERID")?;
				check_container_id(&container_id)?;

				let config: NetworkConfig = serde_json::from_slice(&payload)?;
				Self::check_version(&config.cni_version)?;

				Ok(Self::Add {
					container_id,
					ifname: require_env("CNI_IFNAME")?,
					netns: require_env("CNI_NETNS")?,
					path,
					config,
				})
			}
			Command::Del => {
				let container_id: String = require_env("CNI_CONTAINERID")?;
				check_container_id(&container_id)?;

				let config: NetworkConfig = serde_json::from_slice(&payload)?;
				Self::check_version(&config.cni_version)?;

				Ok(Self::Del {
					container_id,
					ifname: require_env("CNI_IFNAME")?,
					netns: load_env("CNI_NETNS")?,
					path,
					config,
				})
			}
			Command::Check => {
				let container_id: String = require_env("CNI_CONTAINERID")?;
				check_container_id(&container_id)?;

				let config: NetworkConfig = serde_json::from_slice(&payload)?;
				Self::check_version(&config.cni_version)?;

				Ok(Self::Check {
					container_id,
					ifname: require_env("CNI_IFNAME")?,
					netns: require_env("CNI_NETNS")?,
					path,
					config,
				})
			}
			Command::Version => {
				let config: VersionPayload = serde_json::from_slice(&payload)?;
				Ok(Self::Version(config.cni_version))
			}
		}
	}

	pub fn load() -> Self {
		debug!(
			"CNI plugin built with {} crate version {}",
			env!("CARGO_PKG_NAME"),
			env!("CARGO_PKG_VERSION")
		);

		let cni_version = Version::parse("1.0.0").unwrap();

		match Self::from_env() {
			Err(e) => {
				error!("{}", e);
				reply(e.into_reply(cni_version))
			}
			Ok(Cni::Version(v)) => Self::handle_version(v),
			Ok(c) => c,
		}
	}

	// TODO: parse network config (administrator) files
	// maybe also with something that searches in common locations
}
