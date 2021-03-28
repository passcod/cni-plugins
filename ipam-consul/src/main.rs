use std::{collections::HashMap, net::IpAddr, str::FromStr};

use async_std::task::block_on;
use cni_plugin::{reply, Cni, ErrorResult, IpamSuccessResult};
use serde::Deserialize;
use serde_json::Value;

// TODO: pull config from somewhere...

fn main() {
    match Cni::load() {
        Cni::Add {
            container_id,
            config,
            ..
        } => {
            let alloc_id = if container_id.starts_with("cnitool-") {
                "b0695b87-4077-b4c7-fb94-b9414d070341".into() // testing
            } else {
                container_id
            };

            let ipam = match config.ipam.clone() {
                Some(i) => i,
                None => reply(ErrorResult {
                    cni_version: config.cni_version.clone(),
                    code: 7,
                    msg: "missing field",
                    details: "ipam can't proceed without ipam field".into(),
                }),
            };

            let res: Result<IpamSuccessResult, ErrorResult> = block_on(async move {
                let alloc: Alloc =
                    surf::get(format!("http://coco.nut:4646/v1/allocation/{}", alloc_id))
                        .recv_json()
                        .await
                        .map_err(|err| ErrorResult {
                            cni_version: config.cni_version.clone(),
                            code: 100,
                            msg: "whoops",
                            details: format!("{:?}", err),
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
                        .map(|meta| {
                            meta.get("network-ip").map(|v| {
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
                        })
                        .flatten()
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

                // lookup ipam.subnet as key (with / -> |) in consul kv under ipam/
                // error if not found

                // if no ip, fetch the list under the consul kv and pick a random one
                // assign the container_id to the ip (if new/random ip, use cas=0)
                // if assign fails (ie another cni got the ip), retry up to 3 times

                // if no space in subnet, error

                // return ipam result

                Err(ErrorResult {
                    cni_version: config.cni_version.clone(),
                    code: 100,
                    msg: "dbg",
                    details: format!("{:?} {:?}", ip, group.networks),
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
struct Group {
    pub name: String,
    pub meta: Option<HashMap<String, Value>>,
    #[serde(default)]
    pub networks: Vec<Network>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Network {
    pub mode: String,
}