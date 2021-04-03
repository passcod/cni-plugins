use std::{fs::OpenOptions, path::{Path, PathBuf}};

pub fn install_logger(logname: impl AsRef<Path>) {
	use simplelog::*;

	let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
		LevelFilter::Warn,
		Default::default(),
		TerminalMode::Stderr,
		ColorChoice::Never,
	)];

	if cfg!(any(debug_assertions, feature = "release-logs")) {
		let logdir = if cfg!(debug_assertions) {
			std::env::current_dir().unwrap()
		} else {
			PathBuf::from("/var/log/cni")
		};

		let mut logfile = logdir.join(logname);
		logfile.set_extension("log");

		if let Some(dir) = logfile.parent() {
			std::fs::create_dir_all(dir).unwrap();
		}

		loggers.push(WriteLogger::new(
			LevelFilter::Debug,
			Default::default(),
			OpenOptions::new()
				.append(true)
				.create(true)
				.open(logfile)
				.unwrap(),
		));
	}

	CombinedLogger::init(loggers).unwrap();
}
