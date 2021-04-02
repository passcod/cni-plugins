# CNI: IPAM Delegate: Pool selection from Nomad job

_This is a CNI plugin. To learn more about CNI, see [cni.dev](https://cni.dev)._

- Spec support: =0.4.0 || ^1.0.0
- Platform support: Linux. Others may work but aren't tested.
- Obtain at: https://github.com/passcod/cni-plugins/releases
- License: Apache-2.0 OR MIT

## Overview

This is an IPAM Delegate. See https://github.com/passcod/cni-plugins/tree/main/ipam-delegated.

`ipam-ds-nomad` reads an IP pool selection from a Nomad job.

It does the same thing regardless of CNI command.

It looks up an allocation by its ID, as provided to CNI (the `CNI_CONTAINERID`).
Thus it expects to be run by Nomad itself.

The allocation JSON contains a copy of the job definition, and refers to the
task group the allocation is for. This delegate thus obtains the group's
definition, checks that it is using CNI networking, then extracts required pool
information from the group's `meta` dictionary.

## Configuration

To configure, set this plugin as an IPAM delegate in your network configuration
before the allocation delegate, and add the HTTP addresses of your Nomad servers
to the `ipam.nomad_servers` array:

```json
{
  "ipam": {
    "type": "ipam-delegated",
    "delegates": [
      "ipam-ds-nomad",
      "ipam-da-another"
    ],
    "nomad_servers": ["http://10.20.30.40:4646"]
  }
}
```

The servers will be tried in order, and the first one which responds
successfully will be used for all subsequent requests.

## Job configuration

Example Nomad job:

```hcl
job "job" {
  type = "service"
  datacenters = ["dc1"]

  group "leader" {
    count = 1

    network {
      mode = "cni/example"
    }

    meta {
      network-pool = "pool-name"
      network-ip = "10.0.21.100"
    }

    task {
      // ...
    }
  }

  group "workers" {
    count = 10

    network {
      mode = "cni/example"
    }

    meta {
      network-pool = "pool-name"
    }

    task {
      // ...
    }
  }
```

- `network-pool` (string, required): the pool name.
- `network-ip` (string, optional): a static IP address to request within the pool.

In the above example job, the plugin would run 11 times (one for the leader, and
one for each worker).

## Output

This delegate returns an empty (well, all-defaults) IPAM abbreviated success
result, with an additional key `pools` set to an array of Pool objects. At the
moment only one pool object is set, and only the first `network` section is
checked for CNI usage, but that could change.

The Pool object:

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

If the input's `prevResult` is an IPAM success result, and it includes `ips`,
those are copied over to the output. (Makes deletes work.)

## Log file

Error and warn logs are always copied to STDERR.

The `verbose` flavour logs at debug level to `/var/log/cni/ipam-ds-nomad.log`.

The logging is suitable for investigating issues in production. Note that log
messages may span multiple lines and that their format can change at any time.

The version of the plugin is reported at debug level on startup.

## Errors

The plugin errors when:

- the `ipam.nomad_servers` array is missing, empty, or does not contain URLs.
- no nomad server can be successfully reached.
- the nomad alloc for the CNI container ID does not exist.
- the group in the alloc job definition does not exist.
- the group doesn't have a network block.
- the first network block in the group's `mode` field does not start with `cni/`.
- the group doesn't have a meta block.
- the meta doesn't contain the `network-pool` key, or it's not a string.
- the `network-ip` key, if it exists, is not a string.
