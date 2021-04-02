# CNI: IPAM Delegated

_This is a CNI plugin. To learn more about CNI, see [cni.dev](https://cni.dev)._

- Spec support: =0.4.0 || ^1.0.0
- Platform support: Linux. Others may work but aren't tested.
- Obtain at: https://github.com/passcod/cni-plugins/releases
- License: Apache-2.0 OR MIT

## Overview

`ipam-delegated` lets you create a stack of IPAM plugins which are executed when
this plugin is used. Thus, IPAM "delegates" (sub plugins) can each do one thing
and be composed together rather than having more complex plugins that duplicate
functionality or require advanced configuration to do everything.

To configure, set this plugin as the IPAM plugin in your network configuration,
and add IPAM delegates in the `ipam.delegates` array. The delegates should be in
the `CNI_PATH`, same as any other CNI plugin.

```json
{
  "ipam": {
    "type": "ipam-delegated",
    "delegates": [
      "ipam-ds-hello",
      "ipam-da-world"
    ]
  }
}
```

As per spec, each delegate receives the same configuration, and must output an
IPAM abbreviated success result. If one delegate fails during an `ADD` command,
all delegates up until that point, including the one that failed, are run again
with `DEL`.

Unlike the top-level stacked CNI process, this plugin always runs IPAM delegates
in the order they're defined. This is because it's expected only one delegate in
the stack, the final one, provides the IPs: the ones before it fetch information
in some way that will let the final one do its work.

## Delegate naming convention

Plugins that are meant to be delegates are by convention named `ipam-dX-NAME`,
where `X` is a letter describing their role:

- `a` for IP allocation
- `s` for IP pool selection

## Configuration reference

- `delegates` (array of strings, required): plugin names of the delegates

## Log file

Error and warn logs are always copied to STDERR.

The `verbose` flavour logs at debug level to `/var/log/cni/ipam-delegated.log`.

The logging is suitable for investigating issues in production. Note that log
messages may span multiple lines and that their format can change at any time.

The version of the plugin is reported at debug level on startup.

## Appendix

### Example flow

The delegates shown are fictional.

 1. CNI runtime reads `example.conflist`:

    ```json
    {
      "cniVersion": "1.0.0",
      "name": "example",
      "plugins": [
        {
          "type": "bridge",
          "ipam": {
            "type": "ipam-delegated",
            "delegates": [
              "ipam-ds-hello",
              "ipam-da-world"
            ],
            "hello": "192.168.1.0/24",
            "world": 23
          }
        }
      ]
    }
    ```

    It calls the `bridge` plugin with this network config:

    ```json
    {
      "cniVersion": "1.0.0",
      "name": "example",
      "type": "bridge",
      "ipam": {
        "type": "ipam-delegated",
        "delegates": [
          "ipam-ds-hello",
          "ipam-da-world"
        ],
        "hello": "local-pool",
        "world": 23
      }
    }
    ```

 2. The bridge plugin delegates to the `ipam-delegated` plugin, calling it with
    the exact same input.

 3. The ipam-delegated plugin calls the `ipam-ds-hello` delegate with this exact
    same input.

 4. The ipam-ds-hello delegate returns this IPAM Abbreviated Success Result:

    ```json
    {
      "cniVersion": "1.0.0",
      "ips": [],
      "routes": [],
      "dns": {},
      "pools": [
        { "name": "local-pool", "subnet": "192.168.0.0/24" }
      ]
    }
    ```

    (If it fails to execute or returns with an error result: ipam-delegated
    attempts to run it again with the `DEL` command and the same config, and
    ignores failures.)

 5. The ipam-delegated plugin calls the `ipam-da-world` delegate with the
    original network config, but with the `prevResult` key set to the result of
    the previous delegate:

    ```json
    {
      "cniVersion": "1.0.0",
      "name": "example",
      "type": "bridge",
      "ipam": {
        "type": "ipam-delegated",
        "delegates": [
          "ipam-ds-hello",
          "ipam-da-world"
        ],
        "hello": "local-pool",
        "world": 23
      },
      "prevResult": {
        "cniVersion": "1.0.0",
        "ips": [],
        "routes": [],
        "dns": {},
        "pools": [
          { "name": "local-pool", "subnet": "192.168.0.0/24", "gateway": "192.168.0.1" }
        ]
      }
    }
    ```

 6. The ipam-da-world delegate returns this IPAM Abbreviated Success Result:

    ```json
    {
      "cniVersion": "1.0.0",
      "ips": [
        { "address": "192.168.0.23/24", "gateway": "192.168.0.1" }
      ],
      "routes": [
        { "dst": "0.0.0.0/0", "gw": "192.168.0.1" }
      ],
      "dns": {}
    }
    ```

    (If it fails to execute or returns with an error result: ipam-delegated
    attempts to run again with the `DEL` command: 1/ the `ipam-ds-hello`
    delegate, with the prevResult it gave the `ipam-da-world` delegate, then 2/ the
    `ipam-da-world` delegate with the prevResult _that_ DEL returns.)

 7. The ipam-delegated plugin returns that exact same result.

 8. The bridge plugin does its thing and returns to the runtime.

### Errors

The plugin errors when:

- the `ipam.delegates` array is missing, empty, or does not contain strings.
- any delegate errors (but see example flow).
