# npmgen-core

The library behind [npmgen](https://github.com/GGLinnk/npmgen-rs).

It generates the npm publish tree that ships a prebuilt Rust binary.
The tree is a meta package plus one package per platform, wired through `optionalDependencies` and npm `os`/`cpu` filters.
The `npmgen` command-line tool is a thin wrapper over this crate.

## Usage

Add it as a dependency:

```
cargo add npmgen-core
```

Drive a generation through the builder:

```rust
use npmgen_core::Generator;

Generator::builder()
    .manifest_path("Cargo.toml")
    .out("dist/npm")
    .no_build(true)
    .build()
    .run()?;
```

Package identity is read from the target crate with `cargo metadata`.
Targets, payload, and foreign manifests come from `[package.metadata.npmgen]`, documented in the [main README](https://github.com/GGLinnk/npmgen-rs#configuration).

## Key types

- `Generator` builds and runs a generation; `GeneratorBuilder` configures it.
- `Project` is the resolved target crate; `Config` is its npmgen metadata.
- `Target` is one resolved platform; `TargetResolver` derives the set.
- `BuildDriver` is the build seam, with `CargoDriver` as the default.

## License

MIT. See [LICENSE](https://github.com/GGLinnk/npmgen-rs/blob/main/LICENSE).
