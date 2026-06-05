# npmgen

[![npmgen-cli on crates.io](https://img.shields.io/crates/v/npmgen-cli.svg?label=npmgen-cli)](https://crates.io/crates/npmgen-cli) [![npmgen-core on crates.io](https://img.shields.io/crates/v/npmgen-core.svg?label=npmgen-core)](https://crates.io/crates/npmgen-core) [![docs.rs](https://img.shields.io/docsrs/npmgen-core?label=docs.rs)](https://docs.rs/npmgen-core) [![license](https://img.shields.io/crates/l/npmgen-core.svg)](LICENSE)

Ship a prebuilt Rust binary to npm.

npmgen turns a Rust crate into an npm publish tree.
It writes one meta package plus one package per platform, the layout esbuild and napi-rs use.
The meta package lists every platform package as an optional dependency.
npm then installs only the package that matches the host `os` and `cpu`.

Identity comes from `cargo metadata`, so the version stays in one place: your `Cargo.toml`.

## Install

```
cargo install npmgen-cli
```

This gives you two binaries.
`npmgen` runs standalone.
`cargo-npmgen` powers the `cargo npmgen` subcommand.

## Usage

Build every target and assemble the tree:

```
cargo npmgen
```

By default npmgen publishes what `cargo build` would: the workspace default members, one npm package per binary, named after the binary.
Crates marked `publish = false` are skipped unless you name them with `--package`.

Assemble from binaries you already built, with no compilation:

```
cargo npmgen --no-build
```

Useful flags:

- `--out <dir>` sets the output root, default `dist/npm`.
- `--package <name>` publishes only the named workspace members, repeatable or comma separated.
- `--workspace` publishes every workspace member.
- `--exclude <name>` drops workspace members from the selection, repeatable or comma separated.
- `--bin <name>` publishes only the named binaries, repeatable or comma separated.
- `--pkg-version <v>` overrides the version.
- `--target <key>` keeps only the given platforms, repeatable or comma separated.
- `--builder <cmd>` swaps the build driver, for example `cross` or `cargo-zigbuild`.

Every flag has an environment variant, listed by `npmgen --help`.

## Configuration

Declare the payload under `[package.metadata.npmgen]`.
`[workspace.metadata.npmgen]` sets defaults inherited by every member, the way cargo inherits `[workspace.package]`; a crate's own `[package.metadata.npmgen]` overrides them.

```toml
[package.metadata.npmgen]
launcher = { file = "launch.mjs", bin = "mytool" }
include = ["templates", "README.md"]
manifests = [".claude-plugin/plugin.json"]
```

`include` copies files and folders verbatim.
`manifests` renders foreign manifests, described next.

`launcher` provides the JS shim that resolves and runs the platform binary, and can wire it as the npm `bin`.
Name a `file` to copy your own, or omit it to have npmgen generate the standard one: `launcher = { bin = "mytool" }`.
A generated launcher exits non-zero when no platform binary is installed; set `fail_open = true` for hooks that must not block.

## Foreign manifests

A foreign manifest is a JSON or TOML file that another ecosystem owns, such as a Claude Code plugin manifest.
npmgen does not know its schema.
You write the real file and mark the managed values with `${var}`.

```json
{
  "name": "${name}",
  "version": "${version}",
  "author": { "name": "${author_name}", "email": "${author_email}" }
}
```

npmgen parses the file, replaces each placeholder with a typed value, and writes it back.
The swap happens on the parsed data, never on raw text, so the result is always valid and correctly escaped.

Available variables: `name`, `scope`, `package`, `version`, `description`, `license`, `repository`, `git_url`, `bin`, `author`, `author_name`, `author_email`.

## Targets

npmgen picks the platform set by precedence.

1. The `targets` list in your npmgen config, when present.
2. Otherwise cargo's `[build] target` from `.cargo/config.toml`.
3. Otherwise a default set: Windows, macOS and Linux, each on x64 and arm64.

A `--target` filter narrows whichever set wins.

## Publishing

npmgen writes the tree; publishing is your step.

Each run rebuilds the tree in a staging directory and swaps it into place when complete.
A re-run never leaves files from a previous, differently targeted tree.

Publish the meta package and every platform package, for example with `npm publish` in each directory under the output root.

## GitHub Action

Run npmgen in any workflow with the bundled composite action.

```yaml
- uses: GGLinnk/npmgen-rs@main
  with:
    target: linux-x64,darwin-arm64
    no-build: "true"
```

It installs `npmgen-cli` from crates.io and runs it; every CLI flag maps to an input (`manifest-path`, `out`, `package`, `workspace`, `exclude`, `bin`, `pkg-version`, `tag`, `no-build`, `builder`, `target`), and `version` pins the CLI.
Pin to a release tag or commit SHA once one includes the action.
Set up the Rust toolchain and any cross-compilation targets beforehand; the action does not install Rust.
The generated tree root is exposed as the `out` output.

## Security

npmgen runs `cargo build`, which executes the target crate's build scripts and procedural macros.
Only run npmgen on code you trust.
Use `--no-build` to assemble a tree from binaries you built elsewhere, without compiling untrusted code.

Do not run the GitHub Action on untrusted pull requests with registry tokens or other secrets in scope; restrict publishing to trusted refs such as tags.

Error messages and logs can include absolute file paths, so treat CI logs as you would any build output.

## License

MIT. See [LICENSE](LICENSE).
