use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    env::{self, split_paths, VarError},
    io::{stdin, stdout, Read},
    net::IpAddr,
    path::PathBuf,
    process::exit,
    str::FromStr,
};

use ipnetwork::IpNetwork;
use macaddr::MacAddr6;
use regex::Regex;
use semver::{Version, VersionReq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use thiserror::Error;

pub const COMPATIBLE_VERSIONS: &str = "=0.4.0||^1.0.0";
pub const SUPPORTED_VERSIONS: &[&str] = &["0.4.0", "1.0.0"];

#[derive(Debug, Error)]
pub enum CniError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("plugin does not understand CNI version: {0}")]
    Incompatible(Version),

    #[error("missing input network config")]
    MissingInput,

    #[error("missing environment variable: {var}: {err}")]
    MissingEnv {
        var: &'static str,
        #[source]
        err: VarError,
    },

    #[error("environment variable has invalid format: {var}: {err}")]
    InvalidEnv {
        var: &'static str,
        #[source]
        err: Box<dyn std::error::Error>,
    },
}

#[derive(Clone, Copy, Debug, Error)]
#[error("must not be empty")]
pub struct EmptyValueError;

#[derive(Clone, Debug, Error)]
#[error("must match regex: {0}")]
pub struct RegexValueError(Regex);

#[derive(Clone, Copy, Debug)]
enum Command {
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

#[derive(Clone, Debug, Default)]
struct CniArgs(pub HashMap<String, String>);

#[derive(Clone, Copy, Debug, Error)]
#[error("must be in K=V;L=W format")]
pub struct InvalidArgsError;

impl FromStr for CniArgs {
    type Err = InvalidArgsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            s.split(';')
                .filter_map(|p| {
                    let pair: Vec<&str> = p.splitn(2, '=').collect();
                    match pair.as_slice() {
                        [""] => None,
                        [head, tail] => Some(Ok((head.to_string(), tail.to_string()))),
                        _ => Some(Err(InvalidArgsError)),
                    }
                })
                .collect::<Result<_, InvalidArgsError>>()?,
        ))
    }
}

#[derive(Clone, Debug, Default)]
struct CniPath(pub Vec<PathBuf>);

impl FromStr for CniPath {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(split_paths(s).map(PathBuf::from).collect()))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
    #[serde(deserialize_with = "deserialize_version")]
    pub cni_version: Version,
    pub name: String,
    #[serde(rename = "type")]
    pub plugin: String,
    #[serde(default)]
    pub args: HashMap<String, Value>,
    #[serde(default)]
    pub ip_masq: bool,
    #[serde(default)]
    pub ipam: Option<IpamConfig>,
    #[serde(default)]
    pub dns: Option<DnsConfig>,

    #[serde(default, rename = "runtimeConfig")]
    pub runtime: Option<RuntimeConfig>,

    #[serde(default)]
    pub prev_result: Option<Value>,

    #[serde(flatten)]
    pub specific: HashMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpamConfig {
    #[serde(rename = "type")]
    pub plugin: String,

    // doc: common keys, but not in standard
    #[serde(default)]
    pub subnet: Option<IpNetwork>,
    #[serde(default)]
    pub gateway: Option<IpAddr>,
    #[serde(default)]
    pub routes: Vec<Route>,

    #[serde(flatten)]
    pub specific: HashMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub dst: IpNetwork,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gw: Option<IpAddr>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsConfig {
    #[serde(default)]
    pub nameservers: Vec<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub search: Vec<String>,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfig {
    #[serde(default)]
    pub port_mappings: Vec<PortMapping>,
    #[serde(default)]
    pub ips_ranges: Vec<Vec<IpRange>>,
    #[serde(default)]
    pub bandwidth: Option<BandwidthLimits>,
    #[serde(default)]
    pub dns: Option<DnsConfig>,
    #[serde(default)]
    pub ips: Vec<IpNetwork>,
    #[serde(default)]
    pub mac: Option<MacAddr6>,
    #[serde(default)]
    pub aliases: Vec<String>,

    // TODO: infinibandGUID (behind feature)
    // TODO: (PCI) deviceID (behind feature)

    // TODO: in doc, note that entries in specific may get hoisted to fields in future (breaking) versions
    #[serde(flatten)]
    pub specific: HashMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
    #[serde(default)]
    pub protocol: Option<PortProtocol>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortProtocol {
    Tcp,
    Udp,
}

// TODO: enforce all addresses being of the same type
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpRange {
    pub subnet: IpNetwork,
    #[serde(default)]
    pub range_start: Option<IpAddr>,
    #[serde(default)]
    pub range_end: Option<IpAddr>,
    #[serde(default)]
    pub gateway: Option<IpAddr>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandwidthLimits {
    #[serde(default)]
    pub ingress_rate: Option<usize>, // bits per second
    #[serde(default)]
    pub ingress_burst: Option<usize>, // bits
    #[serde(default)]
    pub egress_rate: Option<usize>, // bits per second
    #[serde(default)]
    pub egress_burst: Option<usize>, // bits
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionPayload {
    #[serde(deserialize_with = "deserialize_version")]
    pub cni_version: Version,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionResult {
    #[serde(serialize_with = "serialize_version")]
    pub cni_version: Version,
    #[serde(serialize_with = "serialize_version_list")]
    pub supported_versions: Vec<Version>,
}

impl ResultPayload for VersionResult {}

fn serialize_version<S>(version: &Version, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    version.to_string().serialize(serializer)
}

fn serialize_version_list<S>(list: &Vec<Version>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    list.iter()
        .map(Version::to_string)
        .collect::<Vec<String>>()
        .serialize(serializer)
}

fn deserialize_version<'de, D>(deserializer: D) -> Result<Version, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let j = String::deserialize(deserializer)?;
    Version::from_str(&j).map_err(Error::custom)
}

pub trait ResultPayload {
    fn code(&self) -> i32 {
        0
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResult {
    #[serde(serialize_with = "serialize_version")]
    pub cni_version: Version,
    pub code: i32,
    pub msg: &'static str,
    pub details: String,
}

impl ResultPayload for ErrorResult {
    fn code(&self) -> i32 {
        self.code
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSuccessResult {
    #[serde(serialize_with = "serialize_version")]
    pub cni_version: Version,
    pub interfaces: Vec<InterfaceResult>,
    pub ips: Vec<IpResult>,
    pub routes: Vec<Route>,
    pub dns: DnsResult,
}

impl ResultPayload for AddSuccessResult {}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpamSuccessResult {
    #[serde(serialize_with = "serialize_version")]
    pub cni_version: Version,
    pub ips: Vec<IpResult>,
    pub routes: Vec<Route>,
    pub dns: DnsResult,
}

impl ResultPayload for IpamSuccessResult {}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceResult {
    pub name: String,
    pub mac: MacAddr6,
    pub sandbox: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpResult {
    pub address: IpNetwork,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<IpAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<usize>, // None for ipam
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsResult {
    pub nameservers: Vec<IpAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    pub search: Vec<String>,
    pub options: Vec<String>,
}

pub fn reply<T>(result: T) -> !
where
    T: Serialize + ResultPayload,
{
    serde_json::to_writer(stdout(), &result)
        .expect("Error writing result to stdout... chances are you won't get this either");

    exit(result.code());
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
        stdin().read_to_end(&mut payload)?;
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
        let cni_version = Version::parse("1.0.0").unwrap();

        match Self::from_env() {
            Err(CniError::Io(e)) => {
                reply(ErrorResult {
                    cni_version,
                    code: 5,
                    msg: "I/O error",
                    details: e.to_string(),
                });
            }
            Err(CniError::Json(e)) => {
                reply(ErrorResult {
                    cni_version,
                    code: 6,
                    msg: "Cannot decode JSON payload",
                    details: e.to_string(),
                });
            }
            Err(e @ CniError::Incompatible(_)) => {
                reply(ErrorResult {
                    cni_version,
                    code: 1,
                    msg: "Incompatible CNI version",
                    details: e.to_string(),
                });
            }
            Err(e @ CniError::MissingInput) => {
                reply(ErrorResult {
                    cni_version,
                    code: 7,
                    msg: "Missing payload",
                    details: e.to_string(),
                });
            }
            Err(e @ CniError::MissingEnv { .. }) => {
                reply(ErrorResult {
                    cni_version,
                    code: 4,
                    msg: "Missing environment variable",
                    details: e.to_string(),
                });
            }
            Err(e @ CniError::InvalidEnv { .. }) => {
                reply(ErrorResult {
                    cni_version,
                    code: 4,
                    msg: "Invalid environment variable",
                    details: e.to_string(),
                });
            }
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
}
