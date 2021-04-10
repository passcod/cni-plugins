# CNI: IPAM Delegate: Pool selection from config

_This is a CNI plugin. To learn more about CNI, see [cni.dev](https://cni.dev)._

- Spec support: =0.4.0 || ^1.0.0
- Platform support: Linux. Others may work but aren't tested.
- Obtain at: https://github.com/passcod/cni-plugins/releases
- License: Apache-2.0 OR MIT

## Overview

This is an IPAM Delegate. See https://github.com/passcod/cni-plugins/tree/main/ipam-delegated.

`ipam-ds-static` reads an IP pool selection from the network config;

It does the same thing regardless of CNI command.

## Configuration

To configure, set this plugin as an IPAM delegate in your network configuration
before the allocation delegate, and add the relevant pool settings to your
network configuration:

```json
{
  "ipam": {
    "type": "ipam-delegated",
    "delegates": [
      "ipam-ds-nomad",
      "ipam-da-another"
    ],
    "pools": [
      { "name": "my-pool" }
    ]
  }
}
```

The `ipam.pools` array should likely contain Pool objects, as described:

```json
{
  "name": "pool-name",
  "requested-ip": "10.0.21.123"
}
```

- `name` (string, required): the pool name, as defined by whatever your
  allocation delegate does.
- `requested-ip` (string, optional): a static IP to be allocated from the pool,
  or however your allocation delegate behaves.

## Output

This delegate returns an empty (well, all-defaults) IPAM abbreviated success
result, with an additional key `pools` set to the contents of the `ipam.pools`
array.

If the input's `prevResult` is an IPAM success result, and it includes `ips`,
those are copied over to the output. (Makes deletes work.)

## Log file

Error and warn logs are always copied to STDERR.

The `verbose` flavour logs at debug level to `/var/log/cni/ipam-ds-static.log`.

The logging is suitable for investigating issues in production. Note that log
messages may span multiple lines and that their format can change at any time.

The version of the plugin is reported at debug level on startup.

## Errors

The plugin errors when:

- the `ipam.pools` array is missing.
