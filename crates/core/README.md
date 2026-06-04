# npmgen-core

[![crates.io](https://img.shields.io/crates/v/npmgen-core.svg)](https://crates.io/crates/npmgen-core)
[![docs.rs](https://img.shields.io/docsrs/npmgen-core)](https://docs.rs/npmgen-core)
[![license](https://img.shields.io/crates/l/npmgen-core.svg)](https://github.com/GGLinnk/npmgen-rs/blob/main/LICENSE)

The library behind [npmgen](https://github.com/GGLinnk/npmgen-rs).

It generates the npm publish tree that ships a prebuilt Rust binary.
The tree is a meta package plus one package per platform, wired through `optionalDependencies` and npm `os`/`cpu` filters.
The `npmgen` command-line tool is a thin wrapper over this crate.

Add it as a dependency:

```
cargo add npmgen-core
```

## Two ways in

Acquiring the inputs and generating are separate concerns.
A `Generator` runs over a resolved `Project`; how you obtain that `Project` is up to you.

Load it from a crate manifest (uses `cargo metadata` and TOML):

```rust
use npmgen_core::{Generator, Overrides, Project};

let project = Project::load("Cargo.toml".as_ref(), &Overrides::default())?;
Generator::new(&project).out("dist/npm").run()?;
```

Or build it in memory, with no manifest, no `cargo metadata`, and no TOML parsing:

```rust
use npmgen_core::{Config, Generator, Project};

let project = Project::builder("@me", "mytool", "1.2.3")
    .git_url("git+https://github.com/me/mytool.git")
    .config(Config::default())
    .workspace_root("/path/to/project")
    .target_directory("/path/to/target")
    .build()?;
Generator::new(&project).out("dist/npm").run()?;
```

Targets, payload, and foreign manifests live in `Config`, documented as `[package.metadata.npmgen]` in the [main README](https://github.com/GGLinnk/npmgen-rs#configuration).

## Key types

- `Generator` configures and runs a generation over a `Project`.
- `Project` is the resolved target crate; build it with `Project::builder` or `Project::load`.
- `Config` is the npmgen metadata; `Target` is one resolved platform.
- `BuildDriver` is the build seam, with `CargoDriver` as the default.

## License

MIT. See [LICENSE](https://github.com/GGLinnk/npmgen-rs/blob/main/LICENSE).
