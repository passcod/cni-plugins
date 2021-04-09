use std::{
	fs::OpenOptions,
	path::{Path, PathBuf},
};

use simplelog::Config;

/// Install the standard logger for plugins.
///
/// This logger always emits `warn` and `error` level messages to STDERR, and
/// emits all messages from `debug` level up to a log file in development and
/// when the **release-logs** feature is enabled.
///
/// In development (when `debug_assertions` are enabled), it logs to the current
/// working directory, and otherwise logs to `/var/log/cni/logname.log`,
/// creating the directory if it does not exist.
///
/// Also see [`install_logger_with_config`], notably to filter off modules.
///
/// # Panics
/// - if the working directory cannot be obtained (in development only);
/// - if the logging directory cannot be created (in development or with the
///   release-logs feature only);
/// - if the logfile cannot be opened (in development or with the release-logs
///   feature only);
/// - if the logger cannot be installed.
pub fn install_logger(logname: impl AsRef<Path>) {
	use simplelog::*;

	let mut config = ConfigBuilder::new();
	config.set_thread_level(LevelFilter::Info);
	config.set_target_level(LevelFilter::Info);

	install_logger_with_config(logname, config.build())
}

/// Install the standard logger for plugins, with configuration.
///
/// This logger has identical behaviour to [`install_logger`], but a custom
/// [`Config`] can be passed in.
pub fn install_logger_with_config(logname: impl AsRef<Path>, config: Config) {
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
			config,
			OpenOptions::new()
				.append(true)
				.create(true)
				.open(logfile)
				.unwrap(),
		));
	}

	CombinedLogger::init(loggers).unwrap();
}
