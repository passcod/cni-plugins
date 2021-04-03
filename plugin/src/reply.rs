use std::{collections::HashMap, io::stdout, net::IpAddr, path::PathBuf, process::exit};

use ipnetwork::IpNetwork;
use log::debug;
use macaddr::MacAddr6;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::version::VersionReply;

pub trait ReplyPayload<'de>: std::fmt::Debug + Serialize + Deserialize<'de> {
	fn code(&self) -> i32 {
		0
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReply<'msg> {
	#[serde(deserialize_with = "crate::version::deserialize_version")]
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,
	pub code: i32,
	pub msg: &'msg str,
	pub details: String,
}

impl<'de> ReplyPayload<'de> for ErrorReply<'de> {
	fn code(&self) -> i32 {
		self.code
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuccessReply {
	#[serde(deserialize_with = "crate::version::deserialize_version")]
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,
	#[serde(default)]
	pub interfaces: Vec<InterfaceReply>,
	#[serde(default)]
	pub ips: Vec<IpReply>,
	#[serde(default)]
	pub routes: Vec<RouteReply>,
	pub dns: DnsReply,

	#[serde(flatten)]
	pub specific: HashMap<String, Value>,
}

impl<'de> ReplyPayload<'de> for SuccessReply {}

impl SuccessReply {
	pub fn into_ipam(self) -> Option<IpamSuccessReply> {
		if self.interfaces.is_empty() {
			Some(IpamSuccessReply {
				cni_version: self.cni_version,
				ips: self.ips,
				routes: self.routes,
				dns: self.dns,
				specific: self.specific,
			})
		} else {
			None
		}
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpamSuccessReply {
	#[serde(deserialize_with = "crate::version::deserialize_version")]
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,
	#[serde(default)]
	pub ips: Vec<IpReply>,
	#[serde(default)]
	pub routes: Vec<RouteReply>,
	#[serde(default)]
	pub dns: DnsReply,

	#[serde(flatten)]
	pub specific: HashMap<String, Value>,
}

impl<'de> ReplyPayload<'de> for IpamSuccessReply {}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceReply {
	pub name: String,
	pub mac: MacAddr6,
	pub sandbox: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpReply {
	pub address: IpNetwork,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gateway: Option<IpAddr>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub interface: Option<usize>, // None for ipam
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsReply {
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub nameservers: Vec<IpAddr>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub domain: Option<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub search: Vec<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub options: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteReply {
	pub dst: IpNetwork,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gw: Option<IpAddr>,
}

pub fn reply<'de, T>(result: T) -> !
where
	T: ReplyPayload<'de>,
{
	debug!("replying with {:#?}", result);
	serde_json::to_writer(stdout(), &result)
		.expect("Error writing result to stdout... chances are you won't get this either");

	exit(result.code());
}
