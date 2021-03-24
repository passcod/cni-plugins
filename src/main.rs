use std::{collections::HashMap, convert::Infallible, env::{self, VarError}, io::{Read, stdin}, path::{MAIN_SEPARATOR, PathBuf}, process::exit, str::FromStr};

use thiserror::Error;

#[derive(Debug, Error)]
enum CniError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("missing environment variable: {var}")]
    MissingEnv { var: &'static str, #[source] err: VarError },

    #[error("environment variable has invalid format: {var}")]
    InvalidEnv { var: &'static str, #[source] err: Box<dyn std::error::Error> },
}

#[derive(Clone, Copy, Debug)]
enum Command {
    Add,
    Del,
    Check,
    Version,
}

#[derive(Clone, Copy, Debug, Error)]
#[error("CNI_COMMAND must be one of ADD, DEL, CHECK, VERSION")]
struct InvalidCommandError;

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

#[derive(Clone, Debug)]
struct CniArgs(pub HashMap<String, String>);

#[derive(Clone, Copy, Debug, Error)]
#[error("CNI_ARGS must be in K=V;L=W format")]
struct InvalidArgsError;

impl FromStr for CniArgs {
    type Err = InvalidArgsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.split(';').map(|p| {
            let pair: Vec<&str> = p.splitn(2, '=').collect();
            match pair.as_slice() {
                [head, tail] => Ok((head.to_string(), tail.to_string())),
                _ => Err(InvalidArgsError)
            }
        }).collect::<Result<_, InvalidArgsError>>()?))
    }
}

#[derive(Clone, Debug)]
struct CniPath(pub Vec<PathBuf>);

impl FromStr for CniPath {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.split(MAIN_SEPARATOR).map(PathBuf::from).collect()))
    }
}

#[derive(Clone, Debug)]
struct Cni {
    pub command: Command,
    pub container_id: String,
    pub netns: PathBuf,
    pub ifname: String,
    pub args: HashMap<String, String>,
    pub path: Vec<PathBuf>,
    pub config: serde_json::Value,
}

impl Cni {
    pub fn from_env() -> Result<Self, CniError> {
        fn load_env<T>(var: &'static str) -> Result<T, CniError>
        where
            T: FromStr,
            T::Err : std::error::Error + 'static,
        {
            env::var(var)
                .map_err(|err| CniError::MissingEnv { var, err })
                .and_then(|val| val.parse().map_err(|err| CniError::InvalidEnv { var, err: Box::new(err) }))
        }

        let args: CniArgs = load_env("CNI_ARGS")?;
        let path: CniPath = load_env("CNI_PATH")?;

        let mut netcon_bytes = Vec::with_capacity(1024);
        stdin().read_to_end(&mut netcon_bytes)?;
        let netcon: serde_json::Value = serde_json::from_slice(&netcon_bytes)?;

        Ok(Self {
            command: load_env("CNI_COMMAND")?,
            container_id: load_env("CNI_CONTAINERID")?,
            netns: load_env("CNI_NETNS")?,
            ifname: load_env("CNI_IFNAME")?,
            args: args.0,
            path: path.0,
            config: netcon,
        })
    }

    pub fn load() -> Self {
        match Self::from_env() {
            Ok(c) => c,
            Err(CniError::Io(e)) => {
                eprintln!("{:?}", e);
                exit(5);
            }
            Err(CniError::Json(e)) => {
                eprintln!("{:?}", e);
                exit(6);
            }
            Err(e @ CniError::MissingEnv { .. }) => {
                eprintln!("{:?}", e);
                exit(4);
            }
            Err(e @ CniError::InvalidEnv { .. }) => {
                eprintln!("{:?}", e);
                exit(4);
            }
        }
    }
}

fn main() {
    let cni = Cni::load();
    eprintln!("{:?}", cni);
    exit(100);
}
