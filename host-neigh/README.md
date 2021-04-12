# CNI: Post Processing: Manage neighbours on the host

_This is a CNI plugin. To learn more about CNI, see [cni.dev](https://cni.dev)._

- Spec support: =0.4.0 || ^1.0.0
- Platform support: Linux.
- Obtain at: https://github.com/passcod/cni-plugins/releases
- License: Apache-2.0 OR MIT

## Overview

`host-neigh` manages neighbour entries on the host based on the outputs of
previous plugins or its own config.

## Configuration

To configure, add this plugin after other plugins. Where it goes will depend on
what you want it to do.

```json
{
  "type": "host-routes",
  "neigh": "expression",
  "tries": 3,
}
```

The `neigh` field should contain a [jq] expression as a string, which should
evaluate to an array of Neigh objects, with these fields:

- `address` (IP address as string, required): the IP of the neighbour.
- `device` (string, required): the device name to add the neighbour to.
- `lladdr` (MAC address or interface name as string, optional for `del`): the
  MAC address of the neighbour, or an interface/device name that will be
  resolved into its MAC address.

Returning an empty array is acceptable.

The jq expression is invoked with the [network config] as input, and is limited
to 1 second running time.

`tries` defines how many times failing actions will be retried. Defaults to 3,
caps out at 10, setting to 0 or an invalid value will use the default.

[jq]: https://stedolan.github.io/jq/
[network config]: https://github.com/containernetworking/cni/blob/master/SPEC.md#section-1-network-configuration-format

## Output

This plugin takes the `prevResult` if present, or an empty / all-defaults one
otherwise, and adds (to) a `hostNeighbours` array containing the Neigh objects
returned by the jq expression. Note that this is not supported by `libcni`,
which will ignore it, so is useful only as debug at this point.

## Deletes

The expression will be invoked in the same way, such that the neighbours can be
cleaned up.

## Log file

Error and warn logs are always copied to STDERR.

The `verbose` flavour logs at debug level to `/var/log/cni/host-routes.log`.

The logging is suitable for investigating issues in production. Note that log
messages may span multiple lines and that their format can change at any time.

The version of the plugin is reported at debug level on startup.

## Errors

The plugin errors when:

- the `neigh` field is missing or not a string.
- it's not a valid jq expression.
- the jq expression errors.
- the jq evaluation times out.
- it evaluates to an invalid structure.
- an `lladdr` field is not a mac address nor an existing interface name.
- an `lladdr` field is an interface name but that device does not have a MAC.
