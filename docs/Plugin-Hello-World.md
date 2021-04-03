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

It's also a good idea to add the log crate:

```
cargo add log
```

[cni-plugin]: https://lib.rs/crate/cni-plugin
[cargo-edit]: https://github.com/killercup/cargo-edit

Then open the `src/main.rs` in your favourite editor.

## Load up CNI

This is the basic structure:

```rust
use cni_plugin::Cni;

fn main() {
    cni_plugin::install_logger("hello-world.log");
    match Cni::load() {
        Cni::Add { container_id, ifname, netns, path, config } => {}
        Cni::Del { container_id, ifname, netns, path, config } => {}
        Cni::Check { container_id, ifname, netns, path, config } => {}
        Cni::Version(_) => unreachable!()
    }
}
```

The `Cni::Version` arm is marked unreachable because the `Cni::load()` call
already takes care of it. You can use `Cni::from_env()` if you want to handle
this and basic validation erroring yourself.

Now all you have to do is fill in each operation!

There's a few more things to add in immediately. At minimum, you'll want to use
the `reply` function to send off output and issue the correct exit code, plus
the reply type(s) you're going to use:

```rust
use cni_plugin::reply::{reply, SuccessReply};
```

If you're making an IPAM plugin, you'd use this instead:

```rust
use cni_plugin::reply::{reply, IpamSuccessReply};
```

## Start off with delete

Start by writing an empty DEL implementation that returns an all-defaults reply:

```rust
match {
    // ...
    Cni::Del { config, .. } => {
        reply(SuccessReply {
            cni_version: config.cni_version,
            interfaces: Default::default(),
            ips: Default::default(),
            routes: Default::default(),
            dns: Default::default(),
            specific: Default::default(),
        });
    }
    // ...
}
```

When the cnitool or runtime hits an error with your plugin, for example due to
the lack of `ADD` implementation, it will try to run the `DEL` command to clean
up before exiting! With this little empty reply, that operation will complete
successfully, and makes it a lot easier to test your `ADD` code.

```
> sudo \
  env CNI_PATH=/opt/cni/bin:$PWD/target/debug \
  cnitool add test /var/run/netns/testing

plugin type="cni-hello-world" failed (add): unexpected end of JSON input

(exit code 1)

> sudo \
  env CNI_PATH=/opt/cni/bin:$PWD/target/debug \
  cnitool del test /var/run/netns/testing

(exit code 0)
```

### Look at the logs!

You added nothing but should already have a bunch of logs! Look at the
`hello-world.log` file in your working directory. You may want to `tail -f` it.

## Add some magic

Now you can write your `ADD` code, and then the inverse in your real `DEL` code.

Finally, implement your `CHECK` code.

I'm not going to be writing real networking configuration code at this juncture,
because it's much too complex for a simple tutorial. If I find something that's
small enough, I'll edit it in!

For now, a few more tips:

### Logging

```
use log::{debug, info, warn, error};
```

Use `debug!` and `info!` for debug logging. Use `warn!` and `error!` for
diagnostic warning and errors. The first two will only appear in the log file in
development and verbose builds, but the last two are always copied to STDERR, so
may be visible through the container runtime.

### Errors can be replied

```rust
use error::CniError;

reply(CniError::MissingField("hello.world").into_reply(config.cni_version));
```

`CniError`s thus get transformed to `ErrorReply`s, and `reply` will exit and set
the correct code by reading it from the `ErrorReply`'s body. That makes error
handling a breeze.

### Use CniError until you can, then make your own

[`CniError`](https://docs.rs/cni-plugin/cni-plugin/error/enum.CniError.html)
has a few variants that are not used by the library itself, but for plugins' use
as common errors:

- `Generic`: takes a String
- `Debug`: takes any type or value that implement `Debug`
- `MissingField`: for when a field is missing in config
- `InvalidField`: for when a field is of invalid type or shape in config

Also useful are `CniError::Io` and `CniError::Json`, which wrap `std::io::Error`
and `serde_json::Error` respectively, with `From` implementations for both.

If or when you outgrow `CniError`, you can make your own. This structure is
recommended:

```rust
use cni_plugin::{error::CniError, reply::ErrorReply};
use semver::Version;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
	#[error(transparent)]
	Cni(#[from] CniError),

	#[error("oh no! the {some} chickens of {essential}ness have escaped via the {details}")]
	CustomError {
		some: usize,
		essential: bool,
		details: String,
	},
}

impl AppError {
	pub fn into_reply(self, cni_version: Version) -> ErrorReply<'static> {
		match self {
			Self::Cni(e) => e.into_reply(cni_version),
			e @ AppError::CustomError { .. } => ErrorReply {
				cni_version,
				code: 114,
				msg: "Something went wrong",
				details: e.to_string(),
			},
		}
	}
}
```

### No async main for us, I'm afraid

Because `reply()` exits the process, you want to have any async runtimes finish
and clean up before you call it. There's two approaches here:

#### Inner block_on

```rust
use cni_plugin::{Cni, error::CniError, reply::{reply, SuccessReply}};

fn main() {
    cni_plugin::install_logger("hello-world.log");
    match Cni::load() {
        Cni::Add { container_id, ifname, netns, path, config } => {
            let cni_version = config.cni_version.clone(); // for error
            let res: Result<SuccessReply, CniError> = block_on(async move {
                // your async code
            });

            match res {
                Ok(res) => reply(res),
                Err(res) => reply(res.into_reply(cni_version)),
            }
        }
        Cni::Del { container_id, ifname, netns, path, config } => {}
        Cni::Check { container_id, ifname, netns, path, config } => {}
        Cni::Version(_) => unreachable!()
    }
}
```

#### Outer loop with `.into_inputs()`

```rust
use cni_plugin::{Cni, Command, Inputs, error::CniError, reply::{reply, SuccessReply}};

fn main() {
    cni_plugin::install_logger("hello-world.log");

    // UNWRAP: None on Version, but Version is handled by load()
    let Inputs {
      command,
      container_id,
      config,
      ..
    } = Cni::load().into_inputs().unwrap();

    let cni_version = config.cni_version.clone(); // for error
    let res: Result<SuccessReply, CniError> = block_on(async move {
        // your async prep

        match command {
            Command::Add => {
              // your async code
            },
            Command::Del => {},
            Command::Check => {},
            Command::Version => unreachable!(),
        }
    });

    match res {
        Ok(res) => reply(res),
        Err(res) => reply(res.into_reply(cni_version)),
    }
}
```
