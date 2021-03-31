use cni_plugin::{error::CniError, reply::ErrorReply};
use semver::Version;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
	#[error(transparent)]
	Cni(#[from] CniError),

	#[error("{remote}::{resource}: {err}")]
	Fetch {
		remote: &'static str,
		resource: &'static str,
		#[source]
		err: Box<dyn std::error::Error>,
	},

	#[error("{remote}::{resource} at {path}: {err}")]
	InvalidResource {
		remote: &'static str,
		resource: &'static str,
		path: String,
		#[source]
		err: Box<dyn std::error::Error>,
	},
}

impl AppError {
	pub fn into_result(self, cni_version: Version) -> ErrorReply<'static> {
		match self {
			Self::Cni(e) => e.into_result(cni_version),
			e @ AppError::Fetch { .. } => ErrorReply {
				cni_version,
				code: 111,
				msg: "Error fetching resource",
				details: e.to_string(),
			},
			e @ AppError::InvalidResource { .. } => ErrorReply {
				cni_version,
				code: 117,
				msg: "Invalid resource",
				details: e.to_string(),
			},
		}
	}
}
