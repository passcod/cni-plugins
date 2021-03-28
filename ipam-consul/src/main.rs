use std::{collections::HashMap, str::FromStr};

use async_std::task::block_on;
use cni_plugin::{Cni, ErrorResult, IpamSuccessResult, reply};
use ipnetwork::IpNetwork;
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

                // TODO: check that group is on CNI networking

                // TODO: first check config.runtime.ips

                let ip = group
                    .meta
                    .map(|meta| meta.get("network-ip").map(|v| IpNetwork::from_str(&v.to_string())))
                    .flatten()
                    .transpose()
                    .map_err(|err| ErrorResult {
                        cni_version: config.cni_version.clone(),
                        code: 100,
                        msg: "failed to parse alloc.group.meta.network-ip",
                        details: err.to_string(),
                    });

                // if ip provided check against the ipam.subnet

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
                    details: format!("{:?}", ip),
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
}
