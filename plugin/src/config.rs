use std::{collections::HashMap, net::IpAddr};

use ipnetwork::IpNetwork;
use macaddr::MacAddr6;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ip_range::IpRange;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
	#[serde(deserialize_with = "crate::version::deserialize_version")]
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,
	pub name: String,
	#[serde(rename = "type")]
	pub plugin: String,
	#[serde(default, skip_serializing_if = "HashMap::is_empty")]
	pub args: HashMap<String, Value>,
	#[serde(default)]
	pub ip_masq: bool,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ipam: Option<IpamConfig>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub dns: Option<DnsConfig>,

	#[serde(
		default,
		rename = "runtimeConfig",
		skip_serializing_if = "Option::is_none"
	)]
	pub runtime: Option<RuntimeConfig>,

	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub prev_result: Option<Value>,

	#[serde(flatten)]
	pub specific: HashMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpamConfig {
	#[serde(rename = "type")]
	pub plugin: String,

	// doc: common keys, but not in standard
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub subnet: Option<IpNetwork>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gateway: Option<IpAddr>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsConfig {
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub nameservers: Vec<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub domain: Option<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub search: Vec<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub options: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfig {
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub port_mappings: Vec<PortMapping>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub ips_ranges: Vec<Vec<IpRange>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub bandwidth: Option<BandwidthLimits>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub dns: Option<DnsConfig>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub ips: Vec<IpNetwork>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub mac: Option<MacAddr6>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub aliases: Vec<String>,

	// TODO: infinibandGUID (behind feature)
	// TODO: (PCI) deviceID (behind feature)

	// TODO: in doc, note that entries in specific may get hoisted to fields in future (breaking) versions
	#[serde(flatten)]
	pub specific: HashMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortMapping {
	pub host_port: u16,
	pub container_port: u16,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub protocol: Option<PortProtocol>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortProtocol {
	Tcp,
	Udp,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BandwidthLimits {
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ingress_rate: Option<usize>, // bits per second
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ingress_burst: Option<usize>, // bits
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub egress_rate: Option<usize>, // bits per second
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub egress_burst: Option<usize>, // bits
}
