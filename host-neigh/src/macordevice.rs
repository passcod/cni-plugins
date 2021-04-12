use std::{convert::TryInto, fmt};

use cni_plugin::{error::CniError, macaddr::MacAddr};
use futures::stream::TryStreamExt;
use log::{debug, info};
use macaddr::MacAddr6;
use rtnetlink::LinkHandle;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum MacOrDevice {
	Mac(MacAddr),
	Device(String),
}

impl fmt::Display for MacOrDevice {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Mac(m) => m.fmt(f),
			Self::Device(d) => d.fmt(f),
		}
	}
}

impl MacOrDevice {
	pub fn as_mac(&self) -> Option<&MacAddr> {
		match self {
			Self::Mac(m) => Some(m),
			_ => None,
		}
	}

	pub async fn resolve(&mut self, nllh: &mut LinkHandle) -> Result<(), CniError> {
		use rtnetlink::packet::rtnl::link::nlas::Nla;

		if let Self::Device(ref name) = self {
			debug!("resolving device {} to mac address", name);

			let mut linklist = nllh.get().set_name_filter(name.clone()).execute();
			if let Some(link) = linklist.try_next().await.map_err(crate::nlerror)? {
				info!("link: {:?}", link.header);
				let addr = link
					.nlas
					.iter()
					.filter_map(|n| {
						if let Nla::Address(bytes) = n {
							Some(bytes.clone())
						} else {
							None
						}
					})
					.next()
					.ok_or_else(|| CniError::Generic(format!("no address on link {}", name)))?;

				let addr: [u8; 6] = addr.try_into().map_err(|addr| {
					CniError::Generic(format!(
						"address of link {} is not 6 bytes: {:?}",
						name, addr
					))
				})?;

				let addr = MacAddr6::from(addr).into();
				debug!("got mac address for device {}: {}", name, addr);
				*self = Self::Mac(addr);

				Ok(())
			} else {
				Err(CniError::Generic(format!(
					"interface not found for name {}",
					name
				)))
			}
		} else {
			Ok(())
		}
	}
}

#[test]
fn test_with_mac() {
	use crate::Neigh;

	let s = Neigh {
		address: "1.2.3.4".parse().unwrap(),
		device: "eth0".into(),
		lladdr: Some(MacOrDevice::Mac(MacAddr(MacAddr6::new(0, 0, 0, 0, 0, 0)))),
	};

	let j = serde_json::json!({
		"address": "1.2.3.4",
		"device": "eth0",
		"lladdr": "00:00:00:00:00:00",
	});

	assert_eq!(serde_json::to_value(&s).unwrap(), j);
	assert_eq!(serde_json::from_value::<Neigh>(j).unwrap(), s);
}

#[test]
fn test_with_device() {
	use crate::Neigh;

	let s = Neigh {
		address: "1.2.3.4".parse().unwrap(),
		device: "eth0".into(),
		lladdr: Some(MacOrDevice::Device("eth1".into())),
	};

	let j = serde_json::json!({
		"address": "1.2.3.4",
		"device": "eth0",
		"lladdr": "eth1",
	});

	assert_eq!(serde_json::to_value(&s).unwrap(), j);
	assert_eq!(serde_json::from_value::<Neigh>(j).unwrap(), s);
}
