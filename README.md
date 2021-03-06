# Adopt-a-crate!
**I am no longer working with Nomad, Consul, Kubernetes, or container runtime sysadmin, and so this repository is not maintained. Feel free to fork!** If you're committed, email me to get the crate name transferred.

[![Crate release version](https://flat.badgen.net/crates/v/cni-plugin)](https://crates.io/crates/cni-plugin)
[![Crate license: Apache 2.0 or MIT](https://flat.badgen.net/badge/license/Apache%202.0%20or%20MIT)][copyright]
![MSRV: latest stable](https://flat.badgen.net/badge/MSRV/latest%20stable/orange)
[![CI status](https://github.com/passcod/cni-plugins/actions/workflows/check.yml/badge.svg)](https://github.com/passcod/cni-plugins/actions/workflows/check.yml)

# CNI Plugins

_A library for writing CNI plugins in Rust, and some plugins built with it._

- Plugins:
  * [host-routes](./host-routes), to add routes to the host
  * [host-neigh](./host-neigh), to add neighbours to the host
  * [ipam-delegated](./ipam-delegated), to stack multiple IPAM plugins
  * [ipam-ds-nomad](./ipam-ds-nomad), a **d**elegated IPAM plugin which
    **s**elects IP pool configuration from a Nomad job's metadata
  * [ipam-ds-static](./ipam-ds-static), a **d**elegated IPAM plugin which
    **s**elects IP pool configuration from the network configuration
  * [ipam-da-consul](./ipam-da-consul), a **d**elegated IPAM plugin which
    **a**llocates IPs from a pool stored in Consul KV
- Guides:
  * [Standard tooling](./docs/Standard-Tooling.md)
  * [Hello world plugin](./docs/Plugin-Hello-World.md)
- **[API documentation][docs]** for the `cni-plugin` crate.
- CNI information on the [cni.dev](https://cni.dev) website.
- [Dual-licensed][copyright] with Apache 2.0 and MIT.

[copyright]: ./COPYRIGHT
[docs]: https://docs.rs/cni-plugin

## Obtain plugins

Note that these plugins should not be considered solid implementations. They
work, and I use them, and eventually they'll surely evolve into robust
software, but for now consider them beta quality, and alpha stability.

### Flavours

The `cni-plugins` library can be built with a feature `release-logs` that
enables verbose logging to a file in release builds, which usually is reserved
for debug (development) builds. Warning/error logs are always copied to stderr.

It's up to each plugin to carry through the feature, but all in this repo do.
The pre-build binary releases available below also come in these two flavours,
with the `-verbose` suffix for productions builds with verbose logging to file.

Logs are appended to `/var/log/cni/name-of-plugin.log` in production, and to
`name-of-plugin.log` in the working directory in development.

### From binary release

The [release tab on GitHub](https://github.com/passcod/noodle/releases).

Builds are available for x86-64 Linux, but adding architectures is often
trivial, so please ask if needed.

### From source

Clone this repo, [install the Rust toolchain](https://rustup.rs), and build
with:

```bash
# Standard production binary
cargo build --release

# Log-enabled production binary
cargo build --release --features release-logs

# Debug binary
cargo build
```
