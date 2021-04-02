# CNI: IPAM Delegate: Pool allocation in Consul KV

_This is a CNI plugin. To learn more about CNI, see [cni.dev](https://cni.dev)._

- Spec support: =0.4.0 || ^1.0.0
- Platform support: Linux. Others may work but aren't tested.
- Obtain at: https://github.com/passcod/cni-plugins/releases
- License: Apache-2.0 OR MIT

## Overview

This is an IPAM Delegate. See https://github.com/passcod/cni-plugins/tree/main/ipam-delegated.

`ipam-da-consul` manages IP pools in Consul KV.

To configure, set this plugin as the final IPAM delegate in your network
configuration, and add the HTTP addresses of your Consul servers to the
`ipam.consul_servers` array:

```json
{
  "ipam": {
    "type": "ipam-delegated",
    "delegates": [
      "ipam-dx-another",
      "ipam-da-consul"
    ],
    "consul_servers": ["http://10.20.30.40:8500"]
  }
}
```

The servers will be tried in order, and the first one which responds
successfully will be used for all subsequent requests.

## KV setup

The following folders and keys should be created in Consul KV:

- `ipam/` folder.
- `ipam/pool-name` key, for each `pool-name` you want to define, containing a
  JSON array of IP Ranges as defined below.
- `ipam/pool-name/` folder, for each `pool-name` you define.

The IP Range object is like the one used by the [host-local] IPAM plugin:

```json
{
  "subnet": "10.0.20.0/23",
  "rangeStart": "10.0.21.100",
  "rangeEnd": "10.0.21.150",
  "gateway": "10.0.20.1"
}
```

- `subnet` (string, required): the subnet for this range, in CIDR notation.
- `rangeStart` (string, optional): where to start allocating (inclusive).
- `rangeEnd` (string, optional): where to stop allocating (inclusive).
- `gateway` (string, optional): the gateway for this range.

[host-local]: https://www.cni.dev/plugins/current/ipam/host-local/

Multiple IP Ranges can be set per pool.

## Required input

This delegate expects its input to include a `prevResult.pools` array containing
Pool objects. **At the moment all but the first Pool is ignored.**

The Pool object:

```json
{
  "name": "pool-name",
  "requested-ip": "10.0.21.123"
}
```

- `name` (string, required): the pool name, as defined in Consul.
- `requested-ip` (string, optional): a static IP to be allocated from the pool.

## Allocation

If there's a `requested-ip`, it is re-allocated to this container. Otherwise,
the next available IP in the pool is used.

On delete, the IP(s) are deallocated from the pool in the input if and only if
the IPs in the pool are allocated to the container being deleted.

Allocation is done by creating the key `ipam/pool-name/ip-address` where the IP
address is without its subnet (e.g. `10.0.21.123`), with the following JSON:

```json
{
  "target": "container-id..."
}
```

Note that this is the container ID as in CNI, which might be the container ID,
pod ID, alloc ID... in the runtime.

## Log file

Error and warn logs are always copied to STDERR.

The `verbose` flavour logs at debug level to `/var/log/cni/ipam-da-consul.log`.

The logging is suitable for investigating issues in production. Note that log
messages may span multiple lines and that their format can change at any time.

The version of the plugin is reported at debug level on startup.

## Errors

The plugin errors when:

- the `ipam.consul_servers` array is missing, empty, or does not contain URLs.
- the `prevResult.pools` array is missing, empty, or does not contain valid Pool
  objects.
- no consul server can be successfully reached.
- the selected pool does not exist in KV.
- the `ipam/pool-name` key does not contain valid IP Range objects.
- any key in the pool folder does not contain a valid Allocation object.
- the `requested-ip` does not fit in the pool selected.
- the pool is full (unless a static pool IP was requested).
- a newly allocated IP already exists on KV when we write it (race condition).
- reads from or writes to KV fail.
