use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

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
	pub meta: Option<HashMap<String, Value>>,
	pub networks: Option<Vec<Network>>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(from = "DeGroup")]
pub struct Group {
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
pub struct Network {
	pub mode: String,
}
