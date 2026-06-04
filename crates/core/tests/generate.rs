//! End-to-end checks: generating from fixtures whose identity mirrors real
//! projects must reproduce the expected publish tree, and the identity/override
//! paths must behave as documented.

use std::fs;
use std::path::{Path, PathBuf};

use npmgen_core::project::ProjectError;
use npmgen_core::{Error, Generator, Overrides, Project};
use serde_json::{Value, json};

const DESCRIPTION: &str = "PreToolUse Bash hook that redirects discouraged shell commands to Claude's dedicated tools and to configured MCP servers.";

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture_crate/Cargo.toml")
}

fn out_dir(slot: &str) -> PathBuf {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(slot);
    let _ = fs::remove_dir_all(&out);
    out
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|_| panic!("missing {}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("invalid json {}", path.display()))
}

fn generate(slot: &str) -> PathBuf {
    let out = out_dir(slot);
    Generator::builder()
        .manifest_path(fixture_manifest())
        .out(&out)
        .no_build(true)
        .build()
        .run()
        .expect("generation succeeds");
    out
}

#[test]
fn meta_package_matches_nocmd() {
    let out = generate("meta");
    let meta = read_json(&out.join("nocmd/package.json"));
    assert_eq!(
        meta,
        json!({
            "name": "@gglinnk/nocmd",
            "version": "0.1.1",
            "description": DESCRIPTION,
            "license": "MIT",
            "author": "Gabriel GRONDIN <gglinnk@protonmail.com>",
            "repository": { "type": "git", "url": "git+https://github.com/gglinnk/nocmd.git" },
            "files": [".claude-plugin", "hooks", "launch.mjs"],
            "optionalDependencies": {
                "@gglinnk/nocmd-darwin-arm64": "0.1.1",
                "@gglinnk/nocmd-darwin-x64": "0.1.1",
                "@gglinnk/nocmd-linux-arm64": "0.1.1",
                "@gglinnk/nocmd-linux-x64": "0.1.1",
                "@gglinnk/nocmd-win32-arm64": "0.1.1",
                "@gglinnk/nocmd-win32-x64": "0.1.1",
            },
            "publishConfig": { "access": "public" },
        })
    );
}

#[test]
fn generated_plugin_manifest_matches_nocmd() {
    let out = generate("plugin");
    let plugin = read_json(&out.join("nocmd/.claude-plugin/plugin.json"));
    assert_eq!(
        plugin,
        json!({
            "name": "nocmd",
            "version": "0.1.1",
            "description": DESCRIPTION,
            "author": { "name": "Gabriel GRONDIN", "email": "gglinnk@protonmail.com" },
            "license": "MIT",
            "keywords": ["hook", "pretooluse", "bash", "mcp", "guard"],
        })
    );
}

#[test]
fn platform_packages_match_nocmd() {
    let out = generate("platforms");

    let linux = read_json(&out.join("nocmd-linux-x64/package.json"));
    assert_eq!(
        linux,
        json!({
            "name": "@gglinnk/nocmd-linux-x64",
            "version": "0.1.1",
            "description": "nocmd binary for linux-x64.",
            "license": "MIT",
            "os": ["linux"],
            "cpu": ["x64"],
            "files": ["nocmd"],
        })
    );

    let windows = read_json(&out.join("nocmd-win32-x64/package.json"));
    assert_eq!(windows["files"], json!(["nocmd.exe"]));
    assert_eq!(windows["os"], json!(["win32"]));

    for key in [
        "win32-x64",
        "win32-arm64",
        "darwin-x64",
        "darwin-arm64",
        "linux-x64",
        "linux-arm64",
    ] {
        assert!(
            out.join(format!("nocmd-{key}/package.json")).is_file(),
            "missing platform package {key}"
        );
    }
}

#[test]
fn verbatim_payload_is_copied() {
    let out = generate("payload");
    assert!(out.join("nocmd/launch.mjs").is_file());
    assert!(out.join("nocmd/hooks/hooks.json").is_file());
}

#[test]
fn version_override_propagates_to_meta_and_platform_pins() {
    let out = out_dir("version-override");
    Generator::builder()
        .manifest_path(fixture_manifest())
        .out(&out)
        .no_build(true)
        .version("9.9.9")
        .build()
        .run()
        .unwrap();

    let meta = read_json(&out.join("nocmd/package.json"));
    assert_eq!(meta["version"], json!("9.9.9"));
    assert_eq!(
        meta["optionalDependencies"]["@gglinnk/nocmd-linux-x64"],
        json!("9.9.9")
    );
    assert_eq!(
        read_json(&out.join("nocmd-linux-x64/package.json"))["version"],
        json!("9.9.9")
    );
}

#[test]
fn matching_tag_succeeds_and_mismatch_errors() {
    let out = out_dir("tag");
    Generator::builder()
        .manifest_path(fixture_manifest())
        .out(&out)
        .no_build(true)
        .tag("v0.1.1")
        .build()
        .run()
        .expect("matching tag");

    let error = Generator::builder()
        .manifest_path(fixture_manifest())
        .out(out_dir("tag-mismatch"))
        .no_build(true)
        .tag("v9.9.9")
        .build()
        .run()
        .unwrap_err();
    assert!(matches!(error, Error::TagMismatch { .. }));
}

#[test]
fn overrides_select_package_and_bin() {
    let bin = Project::load(
        &fixture_manifest(),
        &Overrides {
            bin: Some("other".to_owned()),
            ..Overrides::default()
        },
    )
    .unwrap();
    assert_eq!(bin.bin, "other");
    assert_eq!(bin.package.as_deref(), Some("fixture"));

    let selected = Project::load(
        &fixture_manifest(),
        &Overrides {
            package: Some("fixture".to_owned()),
            ..Overrides::default()
        },
    )
    .unwrap();
    assert_eq!(selected.package.as_deref(), Some("fixture"));

    let error = Project::load(
        &fixture_manifest(),
        &Overrides {
            package: Some("nope".to_owned()),
            ..Overrides::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, ProjectError::PackageNotFound { .. }));
}

#[test]
fn virtual_workspace_uses_workspace_package_identity() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture_workspace/Cargo.toml");
    let out = out_dir("virtual");
    Generator::builder()
        .manifest_path(&manifest)
        .out(&out)
        .no_build(true)
        .build()
        .run()
        .unwrap();

    let meta = read_json(&out.join("vtool/package.json"));
    assert_eq!(meta["name"], json!("@acme/vtool"));
    assert_eq!(meta["version"], json!("2.0.0"));
    assert_eq!(meta["description"], json!("Virtual workspace tool."));
    assert_eq!(meta["author"], json!("Acme Dev <dev@acme.test>"));

    let plugin = read_json(&out.join("vtool/.claude-plugin/plugin.json"));
    assert_eq!(plugin["version"], json!("2.0.0"));
    assert_eq!(
        plugin["author"],
        json!({ "name": "Acme Dev", "email": "dev@acme.test" })
    );
}
