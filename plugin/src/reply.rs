use std::{io::stdout, net::IpAddr, path::PathBuf, process::exit};

use ipnetwork::IpNetwork;
use macaddr::MacAddr6;
use semver::Version;
use serde::Serialize;

use crate::config::Route;

pub trait ReplyPayload {
	fn code(&self) -> i32 {
		0
	}
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReply {
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,
	pub code: i32,
	pub msg: &'static str,
	pub details: String,
}

impl ReplyPayload for ErrorReply {
	fn code(&self) -> i32 {
		self.code
	}
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSuccessReply {
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,
	pub interfaces: Vec<InterfaceReply>,
	pub ips: Vec<IpReply>,
	pub routes: Vec<Route>,
	pub dns: DnsReply,
}

impl ReplyPayload for AddSuccessReply {}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpamSuccessReply {
	#[serde(serialize_with = "crate::version::serialize_version")]
	pub cni_version: Version,
	pub ips: Vec<IpReply>,
	pub routes: Vec<Route>,
	pub dns: DnsReply,
}

impl ReplyPayload for IpamSuccessReply {}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceReply {
	pub name: String,
	pub mac: MacAddr6,
	pub sandbox: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpReply {
	pub address: IpNetwork,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gateway: Option<IpAddr>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub interface: Option<usize>, // None for ipam
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsReply {
	pub nameservers: Vec<IpAddr>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub domain: Option<String>,
	pub search: Vec<String>,
	pub options: Vec<String>,
}

pub fn reply<T>(result: T) -> !
where
	T: Serialize + ReplyPayload,
{
	serde_json::to_writer(stdout(), &result)
		.expect("Error writing result to stdout... chances are you won't get this either");

	exit(result.code());
}
