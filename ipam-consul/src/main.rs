use std::{collections::HashMap, net::IpAddr, str::FromStr};

use async_std::task::block_on;
use cni_plugin::{Cni, CniError, ErrorResult, IpRange, IpamSuccessResult, reply};
use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Deserializer};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Cni(#[from] CniError),

    #[error("can't proceed without {0} field")]
    MissingField(&'static str),

    #[error("{0:?}")]
    Debug(Box<dyn std::fmt::Debug>),

    #[error("{field}: expected {expected}, got: {value:?}")]
    InvalidFieldType {
        field: &'static str,
        expected: &'static str,
        value: Value,
    },

    #[error("{remote}::{resource}: {err}")]
    Fetch {
        remote: &'static str,
        resource: &'static str,
        #[source] err: Box<dyn std::error::Error>,
    },

    #[error("{remote}::{resource} at {path}")]
    MissingResource {
        remote: &'static str,
        resource: &'static str,
        path: String,
    },

    #[error("{remote}::{resource} at {path}: {err}")]
    InvalidResource {
        remote: &'static str,
        resource: &'static str,
        path: String,
        #[source] err: Box<dyn std::error::Error>,
    },

    #[error("{0} does not have any free IP space")]
    PoolFull(String),
}

impl AppError {
    fn into_result(self, cni_version: Version) -> ErrorResult {
        match self {
            Self::Cni(e) => e.into_result(cni_version),
            e @ AppError::Debug(_) => ErrorResult {
                cni_version,
                code: 100,
                msg: "DEBUG",
                details: e.to_string(),
            },
            e @ Self::MissingField(_) => ErrorResult {
                cni_version,
                code: 104,
                msg: "Missing field",
                details: e.to_string(),
            },
            e @ AppError::InvalidFieldType { .. } => ErrorResult {
                cni_version,
                code: 107,
                msg: "Invalid field type",
                details: e.to_string(),
            },
            e @ AppError::Fetch { .. } => ErrorResult {
                cni_version,
                code: 111,
                msg: "Error fetching resource",
                details: e.to_string(),
            },
            e @ AppError::MissingResource { .. } => ErrorResult {
                cni_version,
                code: 114,
                msg: "Missing resource",
                details: e.to_string(),
            },
            e @ AppError::InvalidResource { .. } => ErrorResult {
                cni_version,
                code: 117,
                msg: "Invalid resource",
                details: e.to_string(),
            },
            e @ AppError::PoolFull(_) => ErrorResult {
                cni_version,
                code: 122,
                msg: "Pool is full",
                details: e.to_string(),
            },
        }
    }
}

#[derive(Clone, Debug, Error)]
#[error("{0}")]
struct OtherErr(String);
impl OtherErr {
    fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    fn boxed(s: impl Into<String>) -> Box<Self> {
        Box::new(Self::new(s))
    }
}

type AppResult<T> = Result<T, AppError>;

fn main() {
    match Cni::load() {
        Cni::Add {
            container_id,
            config,
            ..
        } => {
            let cni_version = config.cni_version.clone(); // for error
            let res: AppResult<IpamSuccessResult> = block_on(async move {
                let alloc_id = if container_id.starts_with("cnitool-") {
                    "d3428f56-9480-d309-6343-4ec7feded3b3".into() // testing
                } else {
                    container_id
                };

                let ipam = config.ipam.clone().ok_or(AppError::MissingField("ipam"))?;

                let get_config = |name: &'static str| -> AppResult<&Value> {
                    ipam.specific
                        .get(name)
                        .ok_or(AppError::MissingField("ipam"))
                };

                let config_string = |name: &'static str| -> AppResult<String> {
                    get_config(name)
                        .and_then(|v| {
                            if let Value::String(s) = v {
                                Ok(s.to_owned())
                            } else {
                                Err(AppError::InvalidFieldType {
                                    field: name,
                                    expected: "string",
                                    value: v.clone(),
                                })
                            }
                        })
                };

                let pool_name = config_string("pool")?;
                let consul_url = config_string("consul_url")?;
                let nomad_url = config_string("nomad_url")?;

                // lookup defined pool in consul kv at ipam/<pool name>/
                // error if not found
                // parse as JSON Vec<cni::IpRange>
                let pool = {
                    let keys: Vec<ConsulPair<Vec<IpRange>>> =
                        surf::get(format!("{}/v1/kv/ipam/{}", consul_url, pool_name))
                            .recv_json()
                            .await
                            .map_err(|err| AppError::Fetch {
                                remote: "consul",
                                resource: "pool name",
                                err: err.into(),
                            })?;

                    keys.into_iter()
                        .next()
                        .ok_or(AppError::MissingResource {
                            remote: "consul",
                            resource: "pool",
                            path: format!("ipam/{}", pool_name),
                        })?
                        .parsed_value()
                        .map_err(|err| AppError::InvalidResource {
                            remote: "consul",
                            resource: "pool",
                            path: format!("ipam/{}", pool_name),
                            err: Box::new(err),
                        })?
                };

                let alloc: Alloc = surf::get(format!("{}/v1/allocation/{}", nomad_url, alloc_id))
                    .recv_json()
                    .await
                    .map_err(|err| AppError::Fetch {
                        remote: "nomad",
                        resource: "allocation",
                        err: err.into(),
                    })?;

                let group = alloc.job.task_groups.iter().find(|g| g.name == alloc.task_group).ok_or(AppError::InvalidResource {
                    remote: "nomad",
                    resource: "allocation",
                    path: alloc_id.clone(),
                    err: OtherErr::boxed(format!("alloc {} is for task group {} but its own job definition is missing it", alloc_id, alloc.task_group))
                })?.clone();

                // TODO: enable this
                if false {
                    if let Some(network_mode) = group.networks.first().map(|n| &n.mode) {
                        if !network_mode.starts_with("cni/") {
                            return Err(AppError::InvalidFieldType {
                                field: "alloc.group.networks[0].mode",
                                expected: "cni/<name>",
                                value: network_mode.as_str().into(),
                            });
                        }
                    } else {
                        return Err(AppError::MissingField("alloc.group.networks[0]"));
                    }
                }

                let mut ip = config
                    .runtime
                    .as_ref()
                    .map(|c| c.ips.first().map(|ip| ip.ip()))
                    .flatten();

                if ip.is_none() {
                    ip = group
                        .meta
                        .get("network-ip")
                        .map(|v| {
                            if let Value::String(s) = v {
                                IpAddr::from_str(&s).map_err(|_| AppError::InvalidFieldType {
                                    field: "alloc.group.meta.network-ip",
                                    expected: "IP address",
                                    value: v.clone(),
                                })
                            } else {
                                Err(AppError::InvalidFieldType {
                                    field: "alloc.group.meta.network-ip",
                                    expected: "string",
                                    value: v.clone(),
                                })
                            }
                        })
                        .transpose()?;
                }

                // if let Some(ip) = ip {
                //     if !(pool.subnets...).contains(ip) {
                //         return Err(AppError::TODO {
                //             // Requested IP not in pool
                //             format!(
                //                 "pool {} does not contain requested address {}",
                //                 pool_name, ip
                //             ),
                //         });
                //     }
                // }

                // let pool_known = fetch and parse {consul_url}/v1/kv/ipam/{pool_name}/?recurse

                // if no ip, fetch the list under the consul kv and pick the next one
                let next_ip = pool
                    .iter()
                    .flat_map(|range| range.iter_free())
                    .filter(|ip| todo!("check pool_known"))
                    .next()
                    .ok_or(AppError::PoolFull(pool_name))?;
                // assign the container_id to the ip (if new/random ip, use cas=0)
                // if assign fails (ie another cni got the ip), retry up to 3 times

                // if no space in subnet, error

                // return ipam result

                Err(AppError::Debug(Box::new((pool, ip, group.networks))))
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Alloc {
    pub task_group: String,
    pub job: Job,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Job {
    pub task_groups: Vec<Group>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DeGroup {
    pub name: String,
    pub meta: Option<HashMap<String, Value>>,
    pub networks: Option<Vec<Network>>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(from = "DeGroup")]
struct Group {
    pub name: String,
    pub meta: HashMap<String, Value>,
    pub networks: Vec<Network>,
}
impl From<DeGroup> for Group {
    fn from(de: DeGroup) -> Self {
        Self {
            name: de.name,
            meta: de.meta.unwrap_or_default(),
            networks: de.networks.unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Network {
    pub mode: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ConsulPair<T> {
    pub lock_index: usize,
    pub key: String,
    pub flags: isize,
    pub value: ConsulValue<T>,
    pub create_index: usize,
    pub modify_index: usize,
}

#[derive(Clone, Debug)]
enum ConsulValue<T> {
    String(String),
    Parsed(T),
}

#[derive(Debug, Error)]
enum ConsulError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
}

impl<'de, T> Deserialize<'de> for ConsulValue<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::String(s))
    }
}

impl<T: DeserializeOwned> ConsulPair<T> {
    pub fn parse_value(mut self) -> Result<Self, ConsulError> {
        match self.value {
            ConsulValue::Parsed(_) => Ok(self),
            ConsulValue::String(raw) => {
                let new_value = serde_json::from_slice(&base64::decode(&raw)?)?;
                self.value = ConsulValue::Parsed(new_value);
                Ok(self)
            }
        }
    }

    pub fn parsed_value(self) -> Result<T, ConsulError> {
        self.parse_value().map(|pair| match pair.value {
            ConsulValue::String(_) => unreachable!(),
            ConsulValue::Parsed(v) => v,
        })
    }
}
