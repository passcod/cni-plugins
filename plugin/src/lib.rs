use std::{collections::HashSet, env, fs::{OpenOptions}, io::{stdin, Read}, path::{Path, PathBuf}, str::FromStr};

use log::{debug, error};
use regex::Regex;
use semver::{Version, VersionReq};
use thiserror::Error;

#[cfg(any(feature = "with-smol", feature = "with-tokio"))]
pub use crate::delegation::delegate;
pub use crate::reply::reply;

use crate::config::NetworkConfig;
use crate::error::{CniError, EmptyValueError, RegexValueError};
use crate::path::CniPath;
use crate::version::VersionResult;

pub mod config;
#[cfg(any(feature = "with-smol", feature = "with-tokio"))]
pub mod delegation;
pub mod error;
pub mod ip_range;
pub mod reply;

mod path;
mod version;

pub const COMPATIBLE_VERSIONS: &str = "=0.4.0||^1.0.0";
pub const SUPPORTED_VERSIONS: &[&str] = &["0.4.0", "1.0.0"];

#[derive(Clone, Copy, Debug)]
pub enum Command {
	Add,
	Del,
	Check,
	Version,
}

#[derive(Clone, Copy, Debug, Error)]
#[error("must be one of ADD, DEL, CHECK, VERSION")]
pub struct InvalidCommandError;

impl FromStr for Command {
	type Err = InvalidCommandError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"ADD" => Ok(Self::Add),
			"DEL" => Ok(Self::Del),
			"CHECK" => Ok(Self::Check),
			"VERSION" => Ok(Self::Version),
			_ => Err(InvalidCommandError),
		}
	}
}

impl AsRef<str> for Command {
	fn as_ref(&self) -> &'static str {
		match self {
			Command::Add => "ADD",
			Command::Del => "DEL",
			Command::Check => "CHECK",
			Command::Version => "VERSION",
		}
	}
}

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
				let config: version::VersionPayload = serde_json::from_slice(&payload)?;
				Ok(Self::Version(config.cni_version))
			}
		}
	}

	pub fn load() -> Self {
		let cni_version = Version::parse("1.0.0").unwrap();

		match Self::from_env() {
			Err(e) => {
				error!("{}", e);
				reply(e.into_result(cni_version))
			},
			Ok(Cni::Version(v)) => {
				let mut supported_versions = SUPPORTED_VERSIONS
					.iter()
					.map(|v| Version::parse(*v))
					.collect::<Result<HashSet<_>, _>>()
					.unwrap();

				let supported = Self::check_version(&v).is_ok();
				if supported {
					supported_versions.insert(v.clone());
				}

				reply(VersionResult {
					cni_version: v.clone(),
					supported_versions: supported_versions.into_iter().collect(),
				});
			}
			Ok(c) => c,
		}
	}

	fn check_version(version: &Version) -> Result<(), CniError> {
		if !VersionReq::parse(COMPATIBLE_VERSIONS)
			.unwrap()
			.matches(version)
		{
			Err(CniError::Incompatible(version.clone()))
		} else {
			Ok(())
		}
	}

	// TODO: parse network config (administrator) files
	// maybe also with something that searches in common locations

	// TODO: integrate with which (crate) to search the CNI_PATH
}

pub fn install_logger(logfile: impl AsRef<Path>) {
	use simplelog::*;

	let mut loggers: Vec<Box<dyn SharedLogger>> = vec![
		TermLogger::new(LevelFilter::Warn, Default::default(), TerminalMode::Stderr, ColorChoice::Never)
	];

	if cfg!(any(debug_assertions, feature = "release-logs")) {
		loggers.push(WriteLogger::new(LevelFilter::Debug, Default::default(), OpenOptions::new().append(true).create(true).open(logfile).unwrap()));
	}

	CombinedLogger::init(loggers).unwrap();
}
