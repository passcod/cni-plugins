use std::env::VarError;

use regex::Regex;
use semver::Version;
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
}

impl CniError {
	// doc: result as in ErrorResult, not std's Result
	pub fn into_result(self, cni_version: Version) -> ErrorReply {
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
		}
	}
}

#[derive(Clone, Copy, Debug, Error)]
#[error("must not be empty")]
pub struct EmptyValueError;

#[derive(Clone, Debug, Error)]
#[error("must match regex: {0}")]
pub struct RegexValueError(pub Regex);
