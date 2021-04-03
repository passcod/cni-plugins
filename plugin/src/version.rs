//! Mostly internal types for handling versions.

use std::str::FromStr;

use semver::Version;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::reply::ReplyPayload;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionPayload {
	#[serde(deserialize_with = "deserialize_version")]
	#[serde(serialize_with = "serialize_version")]
	pub cni_version: Version,
}

/// The reply type for `VERSION` commands.
///
/// The spec currently mandates that supported versions are provided as an
/// exhaustive array, but this library hopes to do support according to semver
/// compatibility, so it cheats a bit when rendering this reply within
/// [`Cni::load()`][crate::Cni::load()] and adds the runtime-requested version
/// number to the `supported_versions` field when it is semver-compatible.
///
/// Also see [`crate::SUPPORTED_VERSIONS`] and [`crate::COMPATIBLE_VERSIONS`].
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionReply {
	/// The version specified on input.
	#[serde(deserialize_with = "deserialize_version")]
	#[serde(serialize_with = "serialize_version")]
	pub cni_version: Version,

	/// The versions this plugin supports.
	#[serde(deserialize_with = "deserialize_version_list")]
	#[serde(serialize_with = "serialize_version_list")]
	pub supported_versions: Vec<Version>,
}

impl<'de> ReplyPayload<'de> for VersionReply {}

pub(crate) fn serialize_version<S>(version: &Version, serializer: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
{
	version.to_string().serialize(serializer)
}

pub(crate) fn serialize_version_list<S>(list: &[Version], serializer: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
{
	list.iter()
		.map(Version::to_string)
		.collect::<Vec<String>>()
		.serialize(serializer)
}

pub(crate) fn deserialize_version<'de, D>(deserializer: D) -> Result<Version, D::Error>
where
	D: Deserializer<'de>,
{
	use serde::de::Error;
	let j = String::deserialize(deserializer)?;
	Version::from_str(&j).map_err(Error::custom)
}

pub(crate) fn deserialize_version_list<'de, D>(deserializer: D) -> Result<Vec<Version>, D::Error>
where
	D: Deserializer<'de>,
{
	use serde::de::Error;
	let j = Vec::<String>::deserialize(deserializer)?;
	j.iter()
		.map(|s| Version::from_str(s).map_err(Error::custom))
		.collect()
}
