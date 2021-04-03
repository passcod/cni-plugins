use std::env::VarError;

use regex::Regex;
use semver::Version;
use serde_json::Value;
use thiserror::Error;

use crate::reply::ErrorReply;

#[derive(Debug, Error)]
pub enum CniError {
	#[error(transparent)]
	Io(#[from] std::io::Error),

	#[error(transparent)]
	Json(#[from] serde_json::Error),

	#[error("plugin does not understand CNI version: {0}")]
	Incompatible(Version),

	#[error("missing input network config")]
	MissingInput,

	#[error("missing plugin output")]
	MissingOutput,

	#[error("missing environment variable: {var}: {err}")]
	MissingEnv {
		var: &'static str,
		#[source]
		err: VarError,
	},

	#[error("environment variable has invalid format: {var}: {err}")]
	InvalidEnv {
		var: &'static str,
		#[source]
		err: Box<dyn std::error::Error>,
	},

	#[error("cannot obtain current working directory")]
	NoCwd,

	#[error("missing (or not on CNI_PATH) plugin {name}: {err}")]
	MissingPlugin {
		name: String,
		#[source]
		err: which::Error,
	},

	#[error("with plugin {plugin}: {err}")]
	Delegated { plugin: String, err: Box<Self> },

	// doc: not used in this library, but provided for plugins
	#[error("{0}")]
	Generic(String),

	// doc: not used in this library, but provided for plugins
	#[error("{0:?}")]
	Debug(Box<dyn std::fmt::Debug>),

	// doc: not used in this library, but provided for plugins
	#[error("can't proceed without {0} field")]
	MissingField(&'static str),

	// doc: not used in this library, but provided for plugins
	#[error("{field}: expected {expected}, got: {value:?}")]
	InvalidField {
		field: &'static str,
		expected: &'static str,
		value: Value,
	},
}

impl CniError {
	// doc: result as in ErrorResult, not std's Result
	pub fn into_reply(self, cni_version: Version) -> ErrorReply<'static> {
		match self {
			Self::Io(e) => ErrorReply {
				cni_version,
				code: 5,
				msg: "I/O error",
				details: e.to_string(),
			},
			Self::Json(e) => ErrorReply {
				cni_version,
				code: 6,
				msg: "Cannot decode JSON payload",
				details: e.to_string(),
			},
			e @ Self::Incompatible(_) => ErrorReply {
				cni_version,
				code: 1,
				msg: "Incompatible CNI version",
				details: e.to_string(),
			},
			e @ Self::MissingInput => ErrorReply {
				cni_version,
				code: 7,
				msg: "Missing payload",
				details: e.to_string(),
			},
			e @ Self::MissingOutput => ErrorReply {
				cni_version,
				code: 7,
				msg: "Missing output",
				details: e.to_string(),
			},
			e @ Self::MissingEnv { .. } => ErrorReply {
				cni_version,
				code: 4,
				msg: "Missing environment variable",
				details: e.to_string(),
			},
			e @ Self::InvalidEnv { .. } => ErrorReply {
				cni_version,
				code: 4,
				msg: "Invalid environment variable",
				details: e.to_string(),
			},
			e @ Self::NoCwd => ErrorReply {
				cni_version,
				code: 5,
				msg: "Bad workdir",
				details: e.to_string(),
			},
			e @ Self::MissingPlugin { .. } => ErrorReply {
				cni_version,
				code: 5,
				msg: "Missing plugin",
				details: e.to_string(),
			},
			e @ Self::Delegated { .. } => ErrorReply {
				cni_version,
				code: 5,
				msg: "Delegated",
				details: e.to_string(),
			},
			Self::Generic(s) => ErrorReply {
				cni_version,
				code: 100,
				msg: "ERROR",
				details: s,
			},
			e @ Self::Debug { .. } => ErrorReply {
				cni_version,
				code: 101,
				msg: "DEBUG",
				details: e.to_string(),
			},
			e @ Self::MissingField(_) => ErrorReply {
				cni_version,
				code: 104,
				msg: "Missing field",
				details: e.to_string(),
			},
			e @ Self::InvalidField { .. } => ErrorReply {
				cni_version,
				code: 107,
				msg: "Invalid field",
				details: e.to_string(),
			},
		}
	}
}

#[derive(Clone, Copy, Debug, Error)]
#[error("must not be empty")]
pub struct EmptyValueError;

#[derive(Clone, Debug, Error)]
#[error("must match regex: {0}")]
pub struct RegexValueError(pub Regex);
