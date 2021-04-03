use std::str::FromStr;

use crate::error::InvalidCommandError;

#[derive(Clone, Copy, Debug)]
pub enum Command {
	Add,
	Del,
	Check,
	Version,
}

impl FromStr for Command {
	type Err = InvalidCommandError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"ADD" => Ok(Self::Add),
			"DEL" => Ok(Self::Del),
			"CHECK" => Ok(Self::Check),
			"VERSION" => Ok(Self::Version),
			_ => Err(InvalidCommandError),
		}
	}
}

impl AsRef<str> for Command {
	fn as_ref(&self) -> &'static str {
		match self {
			Command::Add => "ADD",
			Command::Del => "DEL",
			Command::Check => "CHECK",
			Command::Version => "VERSION",
		}
	}
}
