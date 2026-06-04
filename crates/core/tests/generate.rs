//! End-to-end parity check: generating from a fixture whose identity mirrors
//! nocmd must reproduce nocmd's known-good publish tree.

use std::fs;
use std::path::{Path, PathBuf};

use npmgen_core::Generator;
use serde_json::{Value, json};

const DESCRIPTION: &str = "PreToolUse Bash hook that redirects discouraged shell commands to Claude's dedicated tools and to configured MCP servers.";

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|_| panic!("missing {}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("invalid json {}", path.display()))
}

fn generate() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture_crate/Cargo.toml");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("npmtree");
    let _ = fs::remove_dir_all(&out);

    Generator::builder()
        .manifest_path(manifest)
        .out(&out)
        .no_build(true)
        .build()
        .run()
        .expect("generation succeeds");
    out
}

#[test]
fn meta_package_matches_nocmd() {
    let out = generate();
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
    let out = generate();
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
    let out = generate();

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
    let out = generate();
    assert!(out.join("nocmd/launch.mjs").is_file());
    assert!(out.join("nocmd/hooks/hooks.json").is_file());
}
