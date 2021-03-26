use std::{collections::{HashMap, HashSet}, convert::Infallible, env::{self, split_paths, VarError}, io::{stdin, stdout, Read}, path::PathBuf, process::exit, str::FromStr};

use regex::Regex;
use semver::{Version, VersionReq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use thiserror::Error;

// todo figure out something with these two
const COMPATIBLE_VERSIONS: &str = "=0.4.0||^1.0.0";
const SUPPORTED_VERSIONS: &[&str] = &["0.4.0", "1.0.0"];

#[derive(Debug, Error)]
enum CniError {
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
struct EmptyValueError;

#[derive(Clone, Debug, Error)]
#[error("must match regex: {0}")]
struct RegexValueError(Regex);

#[derive(Clone, Copy, Debug)]
enum Command {
    Add,
    Del,
    Check,
    Version,
}

#[derive(Clone, Copy, Debug, Error)]
#[error("must be one of ADD, DEL, CHECK, VERSION")]
struct InvalidCommandError;

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
struct InvalidArgsError;

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
struct NetworkConfig {
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

    #[serde(default)]
    pub prev_result: Option<Value>,

    #[serde(flatten)]
    pub specific: HashMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IpamConfig {
    #[serde(rename = "type")]
    pub plugin: String,

    #[serde(flatten)]
    pub specific: HashMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DnsConfig {
    #[serde(default)]
    pub nameservers: Vec<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub search: Vec<String>,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionPayload {
    #[serde(deserialize_with = "deserialize_version")]
    pub cni_version: Version,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionResult {
    #[serde(serialize_with = "serialize_version")]
    cni_version: Version,
    #[serde(serialize_with = "serialize_version_list")]
    supported_versions: Vec<Version>,
}

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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorResult {
    #[serde(serialize_with = "serialize_version")]
    cni_version: Version,
    code: u8,
    msg: &'static str,
    details: String,
}

fn reply<T>(result: &T) where T: Serialize {
    serde_json::to_writer(stdout(), result).expect("Error writing result to stdout... chances are you won't get this either");
}

#[derive(Clone, Debug)]
enum Cni {
    Add {
        container_id: String,
        ifname: String,
        netns: PathBuf,
        args: HashMap<String, String>,
        path: Vec<PathBuf>,
        config: NetworkConfig,
    },
    Del {
        container_id: String,
        ifname: String,
        netns: Option<PathBuf>,
        args: HashMap<String, String>,
        path: Vec<PathBuf>,
        config: NetworkConfig,
    },
    Check {
        container_id: String,
        ifname: String,
        netns: PathBuf,
        args: HashMap<String, String>,
        path: Vec<PathBuf>,
        config: NetworkConfig,
    },
    Version(Version),
}

impl Cni {
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

        let args: CniArgs = load_env("CNI_ARGS")?.unwrap_or_default();
        let path: CniPath = load_env("CNI_PATH")?.unwrap_or_default();
        let args = args.0;
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
                    args,
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
                    args,
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
                    args,
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
                reply(&ErrorResult {
                    cni_version,
                    code: 5,
                    msg: "I/O error",
                    details: e.to_string(),
                });
                exit(5);
            }
            Err(CniError::Json(e)) => {
                reply(&ErrorResult {
                    cni_version,
                    code: 6,
                    msg: "Cannot decode JSON payload",
                    details: e.to_string(),
                });
                exit(6);
            }
            Err(e @ CniError::Incompatible(_)) => {
                reply(&ErrorResult {
                    cni_version,
                    code: 1,
                    msg: "Incompatible CNI version",
                    details: e.to_string(),
                });
                exit(1);
            }
            Err(e @ CniError::MissingInput) => {
                reply(&ErrorResult {
                    cni_version,
                    code: 7,
                    msg: "Missing payload",
                    details: e.to_string(),
                });
                exit(7);
            }
            Err(e @ CniError::MissingEnv { .. }) => {
                reply(&ErrorResult {
                    cni_version,
                    code: 4,
                    msg: "Missing environment variable",
                    details: e.to_string(),
                });
                exit(4);
            }
            Err(e @ CniError::InvalidEnv { .. }) => {
                reply(&ErrorResult {
                    cni_version,
                    code: 4,
                    msg: "Invalid environment variable",
                    details: e.to_string(),
                });
                exit(4);
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

                reply(&VersionResult {
                    cni_version: v.clone(),
                    supported_versions: supported_versions.into_iter().collect(),
                });

                exit(if supported { 0 } else { 1 });
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
}

fn main() {
    let cni = Cni::load();
    eprintln!("{:?}", cni);
    exit(100);
}
