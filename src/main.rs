use std::{
    collections::HashMap,
    convert::Infallible,
    env::{self, split_paths, VarError},
    io::{stdin, Read},
    path::PathBuf,
    process::exit,
    str::FromStr,
};

use regex::Regex;
use semver::{Version, VersionReq};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use thiserror::Error;

const COMPATIBLE_VERSIONS: &str = "=0.4.0||^1.0.0";

#[derive(Debug, Error)]
enum CniError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("plugin does not understand CNI version: {0}")]
    Incompatible(Version),

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

pub fn deserialize_version<'de, D>(deserializer: D) -> Result<Version, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let j = String::deserialize(deserializer)?;
    Version::from_str(&j).map_err(Error::custom)
}

#[derive(Clone, Debug)]
struct Cni {
    pub command: Command,
    pub container_id: Option<String>,
    pub netns: Option<PathBuf>,
    pub ifname: Option<String>,
    pub args: HashMap<String, String>,
    pub path: Vec<PathBuf>,
    pub config: Option<NetworkConfig>,
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

        let mut netcon_bytes = Vec::with_capacity(1024);
        stdin().read_to_end(&mut netcon_bytes)?;
        let config = if netcon_bytes.is_empty() {
            None
        } else {
            let c: NetworkConfig = serde_json::from_slice(&netcon_bytes)?;

            if !VersionReq::parse(COMPATIBLE_VERSIONS)
                .unwrap()
                .matches(&c.cni_version)
            {
                return Err(CniError::Incompatible(c.cni_version));
            }

            Some(c)
        };

        let container_id: Option<String> = load_env("CNI_CONTAINERID")?;
        if let Some(ref id) = container_id {
            if id.is_empty() {
                return Err(CniError::InvalidEnv {
                    var: "CNI_CONTAINERID",
                    err: Box::new(EmptyValueError),
                });
            }

            let re = Regex::new(r"^[a-z0-9][a-z0-9_.\-]*$").unwrap();
            if !re.is_match(&id) {
                return Err(CniError::InvalidEnv {
                    var: "CNI_CONTAINERID",
                    err: Box::new(RegexValueError(re)),
                });
            }
        }

        Ok(Self {
            command: require_env("CNI_COMMAND")?,
            container_id,
            netns: load_env("CNI_NETNS")?,
            ifname: load_env("CNI_IFNAME")?,
            args: args.0,
            path: path.0,
            config,
        })
    }

    pub fn load() -> Self {
        match Self::from_env() {
            Ok(c) => c,
            Err(CniError::Io(e)) => {
                eprintln!("{}", e);
                exit(5);
            }
            Err(CniError::Json(e)) => {
                eprintln!("{}", e);
                exit(6);
            }
            Err(e @ CniError::Incompatible(_)) => {
                eprintln!("{}", e);
                exit(1);
            }
            Err(e @ CniError::MissingEnv { .. }) => {
                eprintln!("{}", e);
                exit(4);
            }
            Err(e @ CniError::InvalidEnv { .. }) => {
                eprintln!("{}", e);
                exit(4);
            }
        }
    }
}

fn main() {
    let cni = Cni::load();
    eprintln!("{:?}", cni);
    exit(100);
}
