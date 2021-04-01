use std::net::IpAddr;

use cni_plugin::{error::CniError, reply::ErrorReply};
use semver::Version;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
	#[error(transparent)]
	Cni(#[from] CniError),

	#[error(transparent)]
	Url(#[from] url::ParseError),

	#[error("{remote}::{resource}: {err}")]
	Http {
		remote: &'static str,
		resource: &'static str,
		#[source]
		err: Box<dyn std::error::Error>,
	},

	#[error("{remote}::{resource} at {path}")]
	MissingResource {
		remote: &'static str,
		resource: &'static str,
		path: String,
	},

	#[error("{remote}::{resource} at {path}: {err}")]
	InvalidResource {
		remote: &'static str,
		resource: &'static str,
		path: String,
		#[source]
		err: Box<dyn std::error::Error>,
	},

	#[error("{0} does not have any free IP space")]
	PoolFull(String),

	#[error("{pool} cannot contain {ip}")]
	NotInPool { pool: String, ip: IpAddr },

	#[error("consul write failed")]
	ConsulWriteFailed,
}

impl AppError {
	pub fn into_result(self, cni_version: Version) -> ErrorReply<'static> {
		match self {
			Self::Cni(e) => e.into_result(cni_version),
			e @ AppError::Url(_) => ErrorReply {
				cni_version,
				code: 120,
				msg: "Error constructing URL",
				details: e.to_string(),
			},
			e @ AppError::Http { .. } => ErrorReply {
				cni_version,
				code: 111,
				msg: "Error fetching resource",
				details: e.to_string(),
			},
			e @ AppError::MissingResource { .. } => ErrorReply {
				cni_version,
				code: 114,
				msg: "Missing resource",
				details: e.to_string(),
			},
			e @ AppError::InvalidResource { .. } => ErrorReply {
				cni_version,
				code: 117,
				msg: "Invalid resource",
				details: e.to_string(),
			},
			e @ AppError::PoolFull(_) => ErrorReply {
				cni_version,
				code: 122,
				msg: "Pool is full",
				details: e.to_string(),
			},
			e @ AppError::NotInPool { .. } => ErrorReply {
				cni_version,
				code: 124,
				msg: "IP not in pool",
				details: e.to_string(),
			},
			e @ AppError::ConsulWriteFailed => ErrorReply {
				cni_version,
				code: 125,
				msg: "KV PUT",
				details: e.to_string(),
			},
		}
	}
}
