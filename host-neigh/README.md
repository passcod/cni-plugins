# CNI: Post Processing: Manage neighbours on the host

_This is a CNI plugin. To learn more about CNI, see [cni.dev](https://cni.dev)._

- Spec support: =0.4.0 || ^1.0.0
- Platform support: Linux.
- Obtain at: https://github.com/passcod/cni-plugins/releases
- License: Apache-2.0 OR MIT

## Overview

`host-neigh` manages neighbour entries on the host based on the outputs of previous plugins or its own config.

## Configuration

To configure, add this plugin after other plugins. Where it goes will depend on what you want it to do.

```json
{
  "type": "host-routes",
  "neigh": "expression",
}
```

The `neigh` field should contain a [jq](https://stedolan.github.io/jq/) expression as a string, which should evaluate to an array of Neigh objects, with these fields:

- `address` (IP address as string, required): the IP of the neighbour.
- `device` (string, required): the device name to add the neighbour to.
- `lladdr` (MAC address as string, optional for `del`): the MAC address of the neighbour.
- `critical` (boolean, optional): whether failing to apply this should return an error, defaults to `true`.

Returning an empty array is acceptable.

The jq expression is invoked with the [network config](https://github.com/containernetworking/cni/blob/master/SPEC.md#section-1-network-configuration-format) as input, and is limited to 1 second running time.

## Output

This plugin takes the `prevResult` if present, or an empty / all-defaults one otherwise, and adds (to) a `hostNeighbours` array containing the Neigh objects returned by the jq expression.

If non-`critical` actions fail, they will warn, and won't be present in the output, but the plugin will still succeed.

## Deletes

The expression will be invoked in the same way, such that the neighbours can be cleaned up.

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
- any `critical` neighbour entry fails to apply.
