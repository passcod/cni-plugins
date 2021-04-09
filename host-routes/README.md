# CNI: Post Processing: Manage routes on the host

_This is a CNI plugin. To learn more about CNI, see [cni.dev](https://cni.dev)._

- Spec support: =0.4.0 || ^1.0.0
- Platform support: Linux.
- Obtain at: https://github.com/passcod/cni-plugins/releases
- License: Apache-2.0 OR MIT

## Overview

`host-routes` manages routes on the host based on the outputs of previous plugins or its own config.

## Configuration

To configure, add this plugin after other plugins. Where it goes will depend on what you want it to do.

```json
{
  "type": "host-routes",
  "routing": "expression",
}
```

The `routing` field should contain a [jq](https://stedolan.github.io/jq/) expression as a string, which should evaluate to an array of Routing objects, with these fields:

- `prefix` (IP address/subnet as string, required): the routing prefix.
- `device` (string, optional): the device name to route to.
- `gateway` (IP address as string, optional): the gateway to route via.

Returning an empty array is acceptable.

The jq expression is invoked with the [network config](https://github.com/containernetworking/cni/blob/master/SPEC.md#section-1-network-configuration-format) as input, and is limited to 1 second running time.

## Output

This plugin takes the `prevResult` if present, or an empty / all-defaults one otherwise, and adds (to) a `hostRoutes` array containing the Routing objects returned by the jq expression. Note that this is not supported by `libcni`, which will ignore it, so is useful only as debug at this point.

## Deletes

The expression will be invoked in the same way, and should return the same things, such that the routes can be cleaned up.

Failure to remove one route will not prevent the following ones from being removed, but will still return an error.

## Log file

Error and warn logs are always copied to STDERR.

The `verbose` flavour logs at debug level to `/var/log/cni/host-routes.log`.

The logging is suitable for investigating issues in production. Note that log
messages may span multiple lines and that their format can change at any time.

The version of the plugin is reported at debug level on startup.

## Errors

The plugin errors when:

- the `routing` field is missing or not a string.
- it's not a valid jq expression.
- the jq expression errors.
- the jq evaluation times out.
- it evaluates to an invalid structure.
- the routing fail to apply.
