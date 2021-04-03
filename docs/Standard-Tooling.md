# Standard CNI dev tooling

_What you need to create CNI plugins._

## Software

### CNI reference plugins

Your OS should provide a package for these, often called `cni-plugins`. If you
don't have that, you can download them from [the GitHub releases][cni-plugins].

The reference plugins provide well-implemented base building blocks of container
networking, like `bridge`, `macvlan`, etc. While you of course can write a
completely custom CNI stack, often it's a lot easier to get going with a fairly
standard namespaced networking setup and do your custom bits on top.

These are often installed to `/opt/cni/bin` but may also be found in the more
unixy location `/usr/lib/cni`. CNI runtimes don't actually care about the
precise location: you can put them anywhere, so long as the `CNI_PATH` is set
correctly.

[cni-plugins]: https://github.com/containernetworking/plugins/releases

### cnitool

Your OS may provide a package, but it's often better to install that one from
source. You'll need a go compiler, from your OS or [the website][golang], then:

```
go get github.com/containernetworking/cni
go install github.com/containernetworking/cni/cnitool@master
```

[golang]: https://golang.org

`cnitool` uses the standard Go `libcni` to provide a limited CLI tool which can
be used to approximate the workflow a runtime would. Runtimes probably use
`libcni` themselves, so you can be fairly certain things will work.

To use, first create a network namespace:

```
sudo ip netns add testing
```

Then, assuming you have CNI definition at `/etc/cni/net.d/test.conflist`:

```
sudo \
  env CNI_PATH=/opt/cni/bin:/path/to/your/development/cni \
  cnitool add test /var/run/netns/testing
```

You can then go look into the `testing` namespace to inspect network setups e.g.

```
sudo ip -n testing -details link
sudo ip -n testing -details addr
sudo ip -n testing -details route
```

To teardown the CNI setup, use:

```
sudo \
  env CNI_PATH=/opt/cni/bin:/path/to/your/development/cni \
  cnitool del test /var/run/netns/testing
```

Note that this will leave the network namespace in place, as that's the
runtime's responsibility to create/delete, not CNI's. For development purposes,
it also means you can ensure the DEL operation _did_ properly clean everything
up, and of course you can keep testing without having to recreate the namespace.

## Wordware

As in, documentation that you probably should have open while developing.

### The spec

Even if you're developing for a runtime that uses the 0.4.0 spec, just use
version 1.0.0 or higher compatible spec. It's fine. In 99% of cases it will work
out, and you're future-proofing.

At the moment I'm writing, you can use the version off the `master` branch,
later it might have been tagged off. Use the spec version page on the website to
check out the latest and historical versions.

- [SPEC.md in the repo](https://github.com/containernetworking/cni/blob/master/SPEC.md)
- [Spec version page on cni.dev](https://www.cni.dev/docs/spec/)

You'll want to refer directly to the spec when implementing. Notably, sections
2 and 5 are essential, and section 4 is useful when dealing with delegation.

Your favourite library or framework might help and take care of many details,
but at this point in time there's no tooling that will let you avoid reading the
spec completely.

Fortunately, it's a good document, easy to read and understand.

### The conventions doc

The [CONVENTIONS.md] document describes additional semantics which are not part
of the spec, but should be followed when it makes sense.

The most useful part is the Well-Known Capabilities table, for runtimes to
provide additional configuration in an interoperable way, if they so choose.

[CONVENTIONS.md]: https://github.com/containernetworking/cni/blob/master/CONVENTIONS.md

### The reference plugins documentation

Available on the website: https://www.cni.dev/plugins/current/

These are useful for configuring the plugins, but more importantly they are
useful as a guide to how you may structure your own plugins. The documentation
format they use is also a good template, if nothing else than to keep the
ecosystem consistent and the experience predictable for users.
