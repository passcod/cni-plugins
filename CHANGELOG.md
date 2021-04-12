# Changelog

## Next (YYYY-MM-DD)

- Host-Neigh: add ability to resolve an lladdr from a device.

## v0.2.0 (2021-04-10)

- Remove WIP advertise plugin.
- Add host-routes plugin.
- Add host-neigh plugin.
- Add ipam-ds-static plugin.
- Breaking change: fields `reply::Interface.mac` and `config::RuntimeConfig.mac`
  change type to `Option<MacAddr>`.
- Introduce a `MacAddr` type which wraps a `macaddr::MacAddr6` but
  (de)serialises correctly to/from string rather than to/from `[u8; 6]`.
- Compile out trace level logs in release builds for all plugins.
- Breaking change: `install_logger` becomes `logger::install`, and new functions
  are added to `logger` to make it possible to filter modules from the log, plus
  other customisations. [#3](https://github.com/passcod/cni-plugins/issues/3)

## v0.1.0 (2021-04-04)

Initial release
