#![cfg(unix)]
use serde_json::{Value, json};
use softwarefactory::{setup, versions};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
const BINARY: &str = env!("CARGO_BIN_EXE_softwarefactory");
fn cli(prefix: &Path, args: &[&str]) -> Output {
    let mut args = args.to_vec();
    if args.contains(&"update") && !args.contains(&"--to") {
        args.push("--local");
    }
    Command::new(BINARY)
        .args(args)
        .arg("--prefix")
        .arg(prefix)
        .output()
        .unwrap()
}
fn ok(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn legacy(prefix: &Path) {
    let dir = prefix.join("releases/0.1.0");
    fs::create_dir_all(dir.join("bridges")).unwrap();
    fs::copy(BINARY, dir.join("softwarefactory")).unwrap();
    fs::write(dir.join("bridges/notion_review.py"), "legacy review").unwrap();
    fs::write(dir.join("bridges/native_check.py"), "legacy checks").unwrap();
    fs::create_dir_all(prefix.join("bin")).unwrap();
    std::os::unix::fs::symlink(
        dir.join("softwarefactory"),
        prefix.join("bin/softwarefactory"),
    )
    .unwrap();
}
#[test]
fn preview_install_update_rollback_and_repeat() {
    let d = tempfile::tempdir().unwrap();
    let prefix = d.path().join("installation");
    ok(cli(&prefix, &["update"]));
    assert!(!prefix.exists(), "preview must not write anything");
    legacy(&prefix);
    ok(cli(&prefix, &["install", "--apply"]));
    assert_eq!(
        versions::list(&prefix).unwrap().active_version.as_deref(),
        Some("0.1.0")
    );
    ok(cli(&prefix, &["update", "--apply"]));
    let inventory = versions::list(&prefix).unwrap();
    assert_eq!(inventory.active_version.as_deref(), Some(setup::VERSION));
    assert_eq!(inventory.releases.len(), 2);
    assert_eq!(inventory.releases[0].integrity, "verified_local_manifest");
    assert_eq!(inventory.releases[1].integrity, "legacy_without_manifest");
    let output = Command::new(prefix.join("bin/softwarefactory"))
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(
        ok(output).trim(),
        format!("softwarefactory {}", setup::VERSION)
    );
    ok(cli(&prefix, &["update", "--apply"]));
    ok(cli(&prefix, &["install", "--apply"]));
    ok(cli(&prefix, &["update", "--to", "0.1.0", "--apply"]));
    assert_eq!(
        versions::list(&prefix).unwrap().active_version.as_deref(),
        Some("0.1.0")
    );
}
#[test]
fn fresh_install_manifest_detects_tampering_and_does_not_switch() {
    let d = tempfile::tempdir().unwrap();
    legacy(d.path());
    ok(cli(d.path(), &["install", "--apply"]));
    fs::write(
        d.path().join(format!(
            "releases/{}/bridges/native_check.py",
            setup::VERSION
        )),
        "tampered",
    )
    .unwrap();
    let output = cli(d.path(), &["update", "--to", setup::VERSION, "--apply"]);
    assert!(!output.status.success());
    let inventory = versions::list(d.path()).unwrap();
    assert!(inventory.releases[0].integrity.starts_with("invalid:"));
    assert_eq!(inventory.active_version.as_deref(), Some("0.1.0"));
}
#[test]
fn unowned_launcher_and_symlink_directories_are_preserved() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("bin")).unwrap();
    fs::write(d.path().join("bin/softwarefactory"), "user launcher").unwrap();
    assert!(!cli(d.path(), &["update", "--apply"]).status.success());
    assert_eq!(
        fs::read_to_string(d.path().join("bin/softwarefactory")).unwrap(),
        "user launcher"
    );
    assert!(!d.path().join("releases").exists());
    let other = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), other.path().join("releases")).unwrap();
    assert!(!cli(other.path(), &["update", "--apply"]).status.success());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
#[test]
fn external_and_dangling_launchers_are_not_overwritten() {
    for dangling in [false, true] {
        let d = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("0.1.0/softwarefactory");
        if !dangling {
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(BINARY, &target).unwrap();
        }
        fs::create_dir(d.path().join("bin")).unwrap();
        std::os::unix::fs::symlink(&target, d.path().join("bin/softwarefactory")).unwrap();
        assert!(!cli(d.path(), &["update", "--apply"]).status.success());
        assert_eq!(
            fs::read_link(d.path().join("bin/softwarefactory")).unwrap(),
            target
        );
    }
}
#[test]
fn invalid_missing_and_incomplete_releases_fail_without_mutation() {
    let d = tempfile::tempdir().unwrap();
    for target in ["../escape", "latest", "/tmp/escape"] {
        assert!(
            !cli(d.path(), &["update", "--to", target, "--apply"])
                .status
                .success()
        );
    }
    fs::create_dir_all(d.path().join("releases/0.1.0")).unwrap();
    assert!(
        !cli(d.path(), &["update", "--to", "0.1.0", "--apply"])
            .status
            .success()
    );
    assert!(!d.path().join("bin").exists());
}
#[test]
fn stale_preview_and_concurrent_installer_are_rejected() {
    use fs2::FileExt;
    let d = tempfile::tempdir().unwrap();
    legacy(d.path());
    let plan = versions::preview(d.path(), None, true).unwrap();
    fs::remove_file(d.path().join("bin/softwarefactory")).unwrap();
    assert!(versions::apply(&plan).is_err());
    assert!(
        !d.path()
            .join(format!("releases/{}", setup::VERSION))
            .exists()
    );
    let file = fs::File::create(d.path().join(".install.lock")).unwrap();
    file.lock_exclusive().unwrap();
    assert!(!cli(d.path(), &["update", "--apply"]).status.success());
    assert!(
        !d.path()
            .join(format!("releases/{}", setup::VERSION))
            .exists()
    );
}
#[test]
fn machine_commands_do_not_require_a_project() {
    let d = tempfile::tempdir().unwrap();
    let result = cli(
        d.path(),
        &["--project", "/nonexistent/softwarefactory-test", "versions"],
    );
    let inventory: Value = serde_json::from_str(&ok(result)).unwrap();
    assert_eq!(inventory["running_version"], setup::VERSION);
    assert_eq!(inventory["releases"], json!([]));
}
fn old_project(root: &Path) {
    let p = setup::Profile::template("generic", "example");
    setup::apply(&setup::preview(root, &p).unwrap()).unwrap();
    let lock_path = root.join(".product-workflow/lock.json");
    let mut lock = setup::load_lock(root).unwrap();
    lock["core"] = json!("0.1.0");
    let lock_text = setup::json(&lock).unwrap();
    fs::write(&lock_path, &lock_text).unwrap();
    let path = root.join(".product-workflow/ownership.json");
    let mut ownership: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    ownership["files"][".product-workflow/lock.json"] = json!(setup::digest(lock_text.as_bytes()));
    fs::write(path, setup::json(&ownership).unwrap()).unwrap();
}
#[test]
fn project_update_preserves_profile_history_other_project_and_can_rollback() {
    let d = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    old_project(d.path());
    old_project(other.path());
    let profile = fs::read(d.path().join(".product-workflow/profile.json")).unwrap();
    let runs = d.path().join(".product-workflow/runs");
    fs::create_dir_all(&runs).unwrap();
    fs::write(runs.join("pinned.json"), r#"{"core":"0.1.0"}"#).unwrap();
    let plan = setup::update_preview(d.path()).unwrap();
    assert_eq!(setup::load_lock(d.path()).unwrap()["core"], "0.1.0");
    assert!(plan.action.contains("0.1.0 ->"));
    setup::apply(&plan).unwrap();
    assert_eq!(setup::load_lock(d.path()).unwrap()["core"], setup::VERSION);
    assert_eq!(setup::load_lock(other.path()).unwrap()["core"], "0.1.0");
    assert_eq!(
        fs::read(d.path().join(".product-workflow/profile.json")).unwrap(),
        profile
    );
    assert_eq!(
        fs::read_to_string(runs.join("pinned.json")).unwrap(),
        r#"{"core":"0.1.0"}"#
    );
    assert!(setup::update_preview(d.path()).unwrap().changes.is_empty());
    setup::apply(&setup::rollback_preview(d.path(), &plan.id).unwrap()).unwrap();
    assert_eq!(setup::load_lock(d.path()).unwrap()["core"], "0.1.0");
}
#[test]
fn project_update_rejects_manual_edits_and_unknown_schemas() {
    let d = tempfile::tempdir().unwrap();
    old_project(d.path());
    let path = d.path().join(".product-workflow/lock.json");
    let original = fs::read(&path).unwrap();
    let mut lock: Value = serde_json::from_slice(&original).unwrap();
    lock["schema_version"] = json!(99);
    fs::write(&path, setup::json(&lock).unwrap()).unwrap();
    assert!(
        setup::update_preview(d.path())
            .unwrap_err()
            .to_string()
            .contains("Unsupported")
    );
    fs::write(&path, original).unwrap();
    fs::write(
        d.path().join(".product-workflow/context.json"),
        "manual edit",
    )
    .unwrap();
    assert!(setup::update_preview(d.path()).is_err());
}

#[test]
fn different_build_with_same_version_and_missing_manifest_are_rejected() {
    let d = tempfile::tempdir().unwrap();
    ok(cli(d.path(), &["install", "--apply"]));
    let release = d.path().join(format!("releases/{}", setup::VERSION));
    let bridge = release.join("bridges/native_check.py");
    fs::write(&bridge, "different build").unwrap();
    let manifest_path = release.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["sha256"]["bridges/native_check.py"] = json!(setup::digest(b"different build"));
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let output = cli(d.path(), &["update", "--apply"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("different build"));
    fs::remove_file(manifest_path).unwrap();
    assert!(
        !cli(d.path(), &["update", "--to", setup::VERSION, "--apply"])
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(bridge).unwrap(), "different build");
}

#[test]
fn installed_commands_infer_their_prefix_and_reject_conflicting_flags() {
    let d = tempfile::tempdir().unwrap();
    ok(cli(d.path(), &["install", "--apply"]));
    let launcher = d.path().join("bin/softwarefactory");
    let output = Command::new(&launcher).arg("versions").output().unwrap();
    let inventory: Value = serde_json::from_str(&ok(output)).unwrap();
    assert_eq!(
        inventory["prefix"],
        fs::canonicalize(d.path())
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(inventory["active_version"], setup::VERSION);
    ok(Command::new(&launcher)
        .args(["update", "--local", "--apply"])
        .output()
        .unwrap());
    for flags in [
        vec!["update", "--check", "--apply"],
        vec!["update", "--local", "--to", "0.1.0"],
    ] {
        let output = Command::new(&launcher).args(flags).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}
