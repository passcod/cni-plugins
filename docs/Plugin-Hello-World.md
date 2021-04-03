# CNI Plugin: Hello World

_A tutorial / guide to create your first CNI plugin with the cni-plugin crate._

## Preliminaries

You'll need:

- The [standard CNI tooling](./Standard-Tooling.md).
- [Rust](https://rustup.rs).
- Linux with sudo privileges.
- Rust knowledge. This isn’t a Rust beginner introduction.

## Get started

Create a new project:

```
cargo new --bin cni-hello-world
cd cni-hello-world
```

Add the [cni-plugin] crate, either manually editing the `Cargo.toml`, or using
the [cargo-edit] tools:

```
cargo add cni-plugin
```

[cni-plugin]: https://lib.rs/crate/cni-plugin
[cargo-edit]: https://github.com/killercup/cargo-edit

Then open the `src/main.rs` in your favourite editor.

##
