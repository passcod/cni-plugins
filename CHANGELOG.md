# Changelog

## Next (YYYY-MM-DD)

- Removed WIP advertise plugin.
- Added host-routes plugin.
- Added host-neigh plugin.
- Added ipam-ds-static plugin.
- Breaking change: fields `reply::Interface.mac` and `config::RuntimeConfig.mac`
  changed type to `Option<MacAddr>`.
- Introduced a `MacAddr` type which wraps a `macaddr::MacAddr6` but
  (de)serialises correctly to/from string rather than to/from `[u8; 6]`.

## v0.1.0 (2021-04-04)

Initial release
