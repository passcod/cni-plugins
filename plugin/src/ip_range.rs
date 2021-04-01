use std::net::IpAddr;

use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};

// TODO: enforce all addresses being of the same type
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpRange {
	pub subnet: IpNetwork,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub range_start: Option<IpAddr>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub range_end: Option<IpAddr>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gateway: Option<IpAddr>,
}

impl IpRange {
	pub fn iter_free(&self) -> impl Iterator<Item = (IpNetwork, &Self)> {
		let prefix = self.subnet.prefix();
		let range_start = self.range_start;
		let range_end = self.range_end;

		self.subnet
			.iter()
			.filter(move |ip| {
				if let Some(ref start) = range_start {
					if ip < start {
						// TODO: figure out how to START from there instead
						return false;
					}
				}

				if let Some(ref end) = range_end {
					if ip > end {
						// TODO: figure out how to stop the iterator there instead
						return false;
					}
				}

				true
			})
			.map(move |ip| (IpNetwork::new(ip, prefix).unwrap(), self))
		// UNWRAP: panics on invalid prefix, but we got it from another IpNetwork
	}
}
