//! End-to-end checks: generating from fixtures must reproduce the expected
//! publish tree, the identity/override paths must behave as documented, and the
//! engine must run from an in-memory project with no manifest or TOML parsing.

use std::fs;
use std::path::{Path, PathBuf};

use npmgen_core::project::ProjectError;
use npmgen_core::{
    BuildDriver, CompileError, Config, Error, Generator, Launcher, ManifestSpec, NpmError,
    Overrides, Project, Target, TargetSpec,
};
use serde_json::{Value, json};

fn target_spec(key: &str) -> TargetSpec {
    TargetSpec {
        key: key.to_owned(),
        triple: None,
        os: None,
        cpu: None,
    }
}

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

fn load_fixture() -> Project {
    Project::load(&fixture_manifest(), &Overrides::default()).expect("fixture loads")
}

fn generate(slot: &str) -> PathBuf {
    let out = out_dir(slot);
    Generator::new(&load_fixture())
        .out(&out)
        .no_build(true)
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
    let project = Project::load(
        &fixture_manifest(),
        &Overrides {
            version: Some("9.9.9".to_owned()),
            ..Overrides::default()
        },
    )
    .unwrap();
    let out = out_dir("version-override");
    Generator::new(&project)
        .out(&out)
        .no_build(true)
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
    let project = load_fixture();
    Generator::new(&project)
        .out(out_dir("tag"))
        .no_build(true)
        .tag("v0.1.1")
        .run()
        .expect("matching tag");

    let error = Generator::new(&project)
        .out(out_dir("tag-mismatch"))
        .no_build(true)
        .tag("v9.9.9")
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
    let project = Project::load(&manifest, &Overrides::default()).unwrap();
    let out = out_dir("virtual");
    Generator::new(&project)
        .out(&out)
        .no_build(true)
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

#[test]
fn generates_from_an_in_memory_project_without_a_manifest() {
    // A source root with no Cargo.toml, just the launcher payload.
    let source = out_dir("in-memory-src");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("launch.mjs"), "process.exit(0);\n").unwrap();

    let config = Config {
        launcher: Some(Launcher::copied("launch.mjs", None)),
        targets: vec![TargetSpec {
            key: "linux-x64".to_owned(),
            triple: None,
            os: None,
            cpu: None,
        }],
        ..Config::default()
    };
    // No Cargo.toml, no cargo metadata, no TOML parsing, explicit targets.
    let project = Project::builder("@acme", "intool", "3.0.0")
        .git_url("git+https://github.com/acme/intool.git")
        .config(config)
        .workspace_root(source.clone())
        .target_directory(source.join("target"))
        .build()
        .unwrap();

    let out = out_dir("in-memory-out");
    Generator::new(&project)
        .out(&out)
        .no_build(true)
        .run()
        .unwrap();

    let meta = read_json(&out.join("intool/package.json"));
    assert_eq!(meta["name"], json!("@acme/intool"));
    assert_eq!(meta["version"], json!("3.0.0"));
    assert_eq!(
        meta["optionalDependencies"],
        json!({ "@acme/intool-linux-x64": "3.0.0" })
    );
    assert_eq!(meta["files"], json!(["launch.mjs"]));
    assert!(out.join("intool/launch.mjs").is_file());
}

#[test]
fn generates_a_launcher_and_wires_the_bin() {
    let source = out_dir("gen-launcher-src");
    fs::create_dir_all(&source).unwrap();

    let config = Config {
        launcher: Some(Launcher::generated(Some("intool".to_owned()), false)),
        targets: vec![TargetSpec {
            key: "linux-x64".to_owned(),
            triple: None,
            os: None,
            cpu: None,
        }],
        ..Config::default()
    };
    let project = Project::builder("@acme", "intool", "3.0.0")
        .config(config)
        .workspace_root(source.clone())
        .target_directory(source.join("target"))
        .build()
        .unwrap();

    let out = out_dir("gen-launcher-out");
    Generator::new(&project)
        .out(&out)
        .no_build(true)
        .run()
        .unwrap();

    let launcher = out.join("intool/launch.mjs");
    assert!(launcher.is_file(), "launcher was generated");
    let script = fs::read_to_string(&launcher).unwrap();
    assert!(script.contains("spawnSync"));
    assert!(script.contains("process.exit(1);"), "fail-hard by default");

    let meta = read_json(&out.join("intool/package.json"));
    assert_eq!(meta["bin"], json!({ "intool": "launch.mjs" }));
    assert_eq!(meta["files"], json!(["launch.mjs"]));
}

#[test]
fn rerun_into_same_out_removes_orphans_and_leaves_no_staging() {
    let project = load_fixture();
    let out = out_dir("rerun");

    // First run: the default six platforms.
    Generator::new(&project)
        .out(&out)
        .no_build(true)
        .run()
        .unwrap();
    assert!(out.join("nocmd-darwin-x64/package.json").is_file());

    // Second run into the same out, narrowed to one platform.
    Generator::new(&project)
        .out(&out)
        .no_build(true)
        .targets(["linux-x64"])
        .run()
        .unwrap();
    assert!(out.join("nocmd-linux-x64/package.json").is_file());
    assert!(
        !out.join("nocmd-darwin-x64").exists(),
        "orphan platform dir from the first run was removed"
    );

    let leftover_staging = fs::read_dir(out.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("rerun.npmgen-staging")
        });
    assert!(!leftover_staging, "no staging directory was left behind");
}

#[test]
fn out_with_no_final_component_is_rejected_before_any_deletion() {
    let project = load_fixture();
    let error = Generator::new(&project)
        .out(".")
        .no_build(true)
        .run()
        .unwrap_err();
    assert!(matches!(error, Error::Npm(inner) if matches!(*inner, NpmError::InvalidOut { .. })));
}

#[test]
fn renders_a_toml_foreign_manifest_end_to_end() {
    let source = out_dir("toml-src");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("meta.toml"),
        "name = \"${name}\"\nversion = \"${version}\"\n",
    )
    .unwrap();

    let config = Config {
        manifests: vec![ManifestSpec::Path("meta.toml".to_owned())],
        targets: vec![target_spec("linux-x64")],
        ..Config::default()
    };
    let project = Project::builder("@acme", "tt", "4.5.6")
        .config(config)
        .workspace_root(source.clone())
        .target_directory(source.join("target"))
        .build()
        .unwrap();

    let out = out_dir("toml-out");
    Generator::new(&project)
        .out(&out)
        .no_build(true)
        .run()
        .unwrap();

    let rendered = fs::read_to_string(out.join("tt/meta.toml")).unwrap();
    assert!(rendered.contains("name = \"tt\""));
    assert!(rendered.contains("version = \"4.5.6\""));
    assert!(rendered.ends_with('\n'));
}

#[test]
fn generates_a_fail_open_launcher_without_a_bin() {
    let source = out_dir("failopen-src");
    fs::create_dir_all(&source).unwrap();
    let config = Config {
        launcher: Some(Launcher::generated(None, true)),
        targets: vec![target_spec("linux-x64")],
        ..Config::default()
    };
    let project = Project::builder("@acme", "hook", "1.0.0")
        .config(config)
        .workspace_root(source.clone())
        .target_directory(source.join("target"))
        .build()
        .unwrap();

    let out = out_dir("failopen-out");
    Generator::new(&project)
        .out(&out)
        .no_build(true)
        .run()
        .unwrap();

    let script = fs::read_to_string(out.join("hook/launch.mjs")).unwrap();
    assert!(script.contains("process.exit(0);"));
    assert!(!script.contains("process.exit(1);"));

    let meta = read_json(&out.join("hook/package.json"));
    assert!(
        meta.get("bin").is_none(),
        "no bin wired when launcher has none"
    );
}

#[test]
fn places_an_existing_binary_into_its_platform_package() {
    // Stage a fake compiled binary where the assembler expects it.
    let target_dir = out_dir("placed-target");
    let release = target_dir.join("x86_64-unknown-linux-gnu/release");
    fs::create_dir_all(&release).unwrap();
    fs::write(release.join("placed"), b"ELF").unwrap();

    let config = Config {
        targets: vec![target_spec("linux-x64")],
        ..Config::default()
    };
    let project = Project::builder("@acme", "placed", "1.0.0")
        .config(config)
        .workspace_root(out_dir("placed-src"))
        .target_directory(&target_dir)
        .build()
        .unwrap();

    let out = out_dir("placed-out");
    Generator::new(&project)
        .out(&out)
        .no_build(true)
        .run()
        .unwrap();

    let shipped = out.join("placed-linux-x64/placed");
    assert!(shipped.is_file(), "binary copied into the platform package");
    assert_eq!(fs::read(&shipped).unwrap(), b"ELF");
}

/// A build driver that writes a stub binary instead of invoking cargo.
#[derive(Debug)]
struct StubDriver;

impl BuildDriver for StubDriver {
    fn build(&self, project: &Project, target: &Target) -> Result<(), CompileError> {
        let path = target.binary_path(&project.target_directory, &project.bin);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"STUB").unwrap();
        Ok(())
    }
}

#[test]
fn an_injected_build_driver_drives_the_build_phase() {
    let config = Config {
        targets: vec![target_spec("linux-x64")],
        ..Config::default()
    };
    let project = Project::builder("@acme", "inj", "1.0.0")
        .config(config)
        .workspace_root(out_dir("inject-src"))
        .target_directory(out_dir("inject-target"))
        .build()
        .unwrap();

    let out = out_dir("inject-out");
    let driver = StubDriver;
    // No no_build: the build phase runs through the injected driver.
    Generator::new(&project)
        .out(&out)
        .build_driver(&driver)
        .run()
        .unwrap();

    assert_eq!(fs::read(out.join("inj-linux-x64/inj")).unwrap(), b"STUB");
}
