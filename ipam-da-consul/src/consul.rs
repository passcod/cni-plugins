use cni_plugin::error::CniError;
use log::debug;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use surf::Url;
use thiserror::Error;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ConsulPair<T> {
	pub lock_index: usize,
	pub key: String,
	pub flags: isize,
	pub value: ConsulValue<T>,
	pub create_index: usize,
	pub modify_index: usize,
}

#[derive(Clone, Debug)]
pub enum ConsulValue<T> {
	Null,
	String(String),
	Parsed(T),
}

impl<T> ConsulValue<T> {
	pub fn is_null(&self) -> bool {
		matches!(self, Self::Null)
	}
}

#[derive(Debug, Error)]
pub enum ConsulError {
	#[error(transparent)]
	Json(#[from] serde_json::Error),

	#[error(transparent)]
	Base64(#[from] base64::DecodeError),
}

impl<'de, T> Deserialize<'de> for ConsulValue<T> {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = Option::<String>::deserialize(deserializer)?;
		match s {
			None => Ok(Self::Null),
			Some(s) => Ok(Self::String(s)),
		}
	}
}

impl<T: DeserializeOwned> ConsulPair<T> {
	pub fn parse_value(mut self) -> Result<Self, ConsulError> {
		match self.value {
			ConsulValue::Null | ConsulValue::Parsed(_) => Ok(self),
			ConsulValue::String(raw) => {
				let new_value = serde_json::from_slice(&base64::decode(&raw)?)?;
				self.value = ConsulValue::Parsed(new_value);
				Ok(self)
			}
		}
	}

	pub fn parsed_value(self) -> Result<Option<T>, ConsulError> {
		self.parse_value().map(|pair| match pair.value {
			ConsulValue::String(_) => unreachable!(),
			ConsulValue::Parsed(v) => Some(v),
			ConsulValue::Null => None,
		})
	}
}

pub async fn delete_all(
	consul_url: &Url,
	keys: impl Iterator<Item = (String, usize)>,
) -> AppResult<()> {
	#[derive(Clone, Debug, Serialize)]
	enum TxnAction {
		#[serde(rename = "KV")]
		Kv(TxnKv),
	}

	#[derive(Clone, Debug, Serialize)]
	#[serde(rename_all = "PascalCase")]
	struct TxnKv {
		verb: &'static str,
		key: String,
		index: usize,
	}

	let actions = keys
		.map(|(key, index)| {
			TxnAction::Kv(TxnKv {
				verb: "delete-cas",
				key,
				index,
			})
		})
		.collect::<Vec<_>>();

	// FIXME!!! We actually want to best-effort this, not to fail the whole thing
	// on error (because otherwise we'll leave dangling IPs if any in this set
	// are re-allocated before we get to delete them, as is common in rolling
	// update situations). So, need to refactor without the transaction.
	debug!("going to delete {} entries", actions.len());
	let txn_url = consul_url.join("v1/txn")?;
	let res = surf::put(txn_url)
		.body(serde_json::to_value(actions).map_err(CniError::Json)?)
		.await?;

	match res.status().into() {
		200 => Ok(()),
		409 => Err(AppError::ConsulWriteFailed),
		code => Err(CniError::Generic(format!("invalid txn return status: {}", code)).into()),
	}
}
