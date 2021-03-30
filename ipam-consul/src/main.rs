use std::{collections::HashMap, net::IpAddr, str::FromStr};

use async_std::task::block_on;
use cni_plugin::{reply, Cni, ErrorResult, IpRange, IpamSuccessResult};
use serde::{de::DeserializeOwned, Deserialize, Deserializer};
use serde_json::Value;
use thiserror::Error;

fn main() {
    match Cni::load() {
        Cni::Add {
            container_id,
            config,
            ..
        } => {
            let res: Result<IpamSuccessResult, ErrorResult> = block_on(async move {
                let alloc_id = if container_id.starts_with("cnitool-") {
                    "d3428f56-9480-d309-6343-4ec7feded3b3".into() // testing
                } else {
                    container_id
                };

                let ipam = config.ipam.clone().ok_or(ErrorResult {
                    cni_version: config.cni_version.clone(),
                    code: 7,
                    msg: "missing field",
                    details: "ipam can't proceed without ipam field".into(),
                })?;

                let config_string = |name: &'static str| -> Result<String, ErrorResult> {
                    ipam.specific
                        .get(name)
                        .ok_or(ErrorResult {
                            cni_version: config.cni_version.clone(),
                            code: 7,
                            msg: "missing field",
                            details: format!(
                                "ipam-consul can't proceed without ipam.{} field",
                                name
                            ),
                        })
                        .and_then(|v| {
                            if let Value::String(s) = v {
                                Ok(s.to_owned())
                            } else {
                                Err(ErrorResult {
                                    cni_version: config.cni_version.clone(),
                                    code: 7,
                                    msg: "invalid field type",
                                    details: format!("ipam.{}: expected string, got {:?}", name, v),
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
                            .map_err(|err| ErrorResult {
                                cni_version: config.cni_version.clone(),
                                code: 100,
                                msg: "http fail retrieving pool",
                                details: err.to_string(),
                            })?;

                    keys.into_iter()
                        .next()
                        .ok_or(ErrorResult {
                            cni_version: config.cni_version.clone(),
                            code: 100,
                            msg: "missing pool",
                            details: format!("ipam/{} does not exist in consul", pool_name),
                        })?
                        .parsed_value()
                        .map_err(|err| ErrorResult {
                            cni_version: config.cni_version.clone(),
                            code: 100,
                            msg: "invalid pool",
                            details: err.to_string(),
                        })?
                };

                let alloc: Alloc = surf::get(format!("{}/v1/allocation/{}", nomad_url, alloc_id))
                    .recv_json()
                    .await
                    .map_err(|err| ErrorResult {
                        cni_version: config.cni_version.clone(),
                        code: 100,
                        msg: "http fail retrieving alloc",
                        details: err.to_string(),
                    })?;

                let group = alloc.job.task_groups.iter().find(|g| g.name == alloc.task_group).ok_or(ErrorResult {
                    cni_version: config.cni_version.clone(),
                    code: 100,
                    msg: "missing group in alloc",
                    details: format!("alloc {} is for task group {} but its own job definition is missing it", alloc_id, alloc.task_group),
                })?.clone();

                // TODO: enable this
                if false {
                    if let Some(network_mode) = group.networks.first().map(|n| &n.mode) {
                        if !network_mode.starts_with("cni/") {
                            return Err(ErrorResult {
                                cni_version: config.cni_version.clone(),
                                code: 100,
                                msg: "alloc.group.network.mode is not a CNI",
                                details: format!("expected: cni/<name>, got: {}", network_mode),
                            });
                        }
                    } else {
                        return Err(ErrorResult {
                            cni_version: config.cni_version.clone(),
                            code: 100,
                            msg: "alloc.group.network is missing",
                            details: "you can't really have a network without a network".into(),
                        });
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
                                IpAddr::from_str(&s).map_err(|err| ErrorResult {
                                    cni_version: config.cni_version.clone(),
                                    code: 100,
                                    msg: "failed to parse alloc.group.meta.network-ip",
                                    details: format!("{} (ip={:?})", err, s),
                                })
                            } else {
                                Err(ErrorResult {
                                    cni_version: config.cni_version.clone(),
                                    code: 100,
                                    msg: "alloc.group.meta.network-ip is not a string",
                                    details: format!("it appears to be a: {:?}", v),
                                })
                            }
                        })
                        .transpose()?;
                }

                if let (Some(subnet), Some(ip)) = (ipam.subnet, ip) {
                    if !subnet.contains(ip) {
                        return Err(ErrorResult {
                            cni_version: config.cni_version.clone(),
                            code: 100,
                            msg: "network config subnet !! requested ip",
                            details: format!(
                                "network's config subnet {} does not contain requested ip {}",
                                subnet, ip
                            ),
                        });
                    }
                }

                // let pool_known = fetch and parse {consul_url}/v1/kv/ipam/{pool_name}/?recurse

                // if no ip, fetch the list under the consul kv and pick the next one
                let next_ip = pool
                    .iter()
                    .flat_map(|range| range.iter_free())
                    .filter(|ip| todo!("check pool_known"))
                    .next()
                    .ok_or(ErrorResult {
                        cni_version: config.cni_version.clone(),
                        code: 100,
                        msg: "pool is full",
                        details: format!("pool {} does not have any free addresses", pool_name),
                    })?;
                // assign the container_id to the ip (if new/random ip, use cas=0)
                // if assign fails (ie another cni got the ip), retry up to 3 times

                // if no space in subnet, error

                // return ipam result

                Err(ErrorResult {
                    cni_version: config.cni_version.clone(),
                    code: 100,
                    msg: "dbg",
                    details: format!("{:?} {:?} {:?}", pool, ip, group.networks),
                })
            });

            match res {
                Err(res) => reply(res),
                Ok(res) => reply(res),
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
