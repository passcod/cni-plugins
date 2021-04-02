use std::net::IpAddr;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Alloc {
	pub task_group: String,
	pub job: Job,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
	pub task_groups: Vec<Group>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeGroup {
	pub name: String,
	pub meta: Option<Meta>,
	pub networks: Option<Vec<Network>>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(from = "DeGroup")]
pub struct Group {
	pub name: String,
	pub meta: Meta,
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
pub struct Network {
	pub mode: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Meta {
	#[serde(default)]
	pub network_pool: Option<String>,
	#[serde(default)]
	pub network_ip: Option<IpAddr>,
}
