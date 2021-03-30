use std::net::IpAddr;

use ipnetwork::IpNetwork;
use serde::Deserialize;

// TODO: enforce all addresses being of the same type
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpRange {
	pub subnet: IpNetwork,
	#[serde(default)]
	pub range_start: Option<IpAddr>,
	#[serde(default)]
	pub range_end: Option<IpAddr>,
	#[serde(default)]
	pub gateway: Option<IpAddr>,
}

impl IpRange {
	pub fn iter_free(&self) -> impl Iterator<Item = IpNetwork> {
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
			.map(move |ip| IpNetwork::new(ip, prefix).unwrap())
		// UNWRAP: panics on invalid prefix, but we got it from another IpNetwork
	}
}
