use cni_plugin::{error::CniError, reply::ErrorReply};
use semver::Version;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
	#[error(transparent)]
	Cni(#[from] CniError),

	#[error("can't proceed without {0} field")]
	MissingField(&'static str),

	#[error("{0:?}")]
	Debug(Box<dyn std::fmt::Debug>),

	#[error("{field}: expected {expected}, got: {value:?}")]
	InvalidFieldType {
		field: &'static str,
		expected: &'static str,
		value: Value,
	},

	#[error("{remote}::{resource}: {err}")]
	Fetch {
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
}

impl AppError {
	pub fn into_result(self, cni_version: Version) -> ErrorReply {
		match self {
			Self::Cni(e) => e.into_result(cni_version),
			e @ AppError::Debug(_) => ErrorReply {
				cni_version,
				code: 100,
				msg: "DEBUG",
				details: e.to_string(),
			},
			e @ Self::MissingField(_) => ErrorReply {
				cni_version,
				code: 104,
				msg: "Missing field",
				details: e.to_string(),
			},
			e @ AppError::InvalidFieldType { .. } => ErrorReply {
				cni_version,
				code: 107,
				msg: "Invalid field type",
				details: e.to_string(),
			},
			e @ AppError::Fetch { .. } => ErrorReply {
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
		}
	}
}

#[derive(Clone, Debug, Error)]
#[error("{0}")]
pub struct OtherErr(String);
impl OtherErr {
	pub fn new(s: impl Into<String>) -> Self {
		Self(s.into())
	}

	pub fn boxed(s: impl Into<String>) -> Box<Self> {
		Box::new(Self::new(s))
	}
}

pub type AppResult<T> = Result<T, AppError>;
