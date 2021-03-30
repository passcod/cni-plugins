use std::{collections::HashMap, net::IpAddr};

use ipnetwork::IpNetwork;
use macaddr::MacAddr6;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ip_range::IpRange;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
	#[serde(deserialize_with = "crate::version::deserialize_version")]
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
