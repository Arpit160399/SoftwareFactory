use serde_json::json;
use softwarefactory::{adapters, engine::*, setup::*};
use std::{fs, path::Path};
fn project(id: &str) -> (tempfile::TempDir, Profile) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".softwarefactory-synthetic-fixture"),
        "test only",
    )
    .unwrap();
    fs::write(dir.path().join("AGENTS.md"), "Preserve other files.\n").unwrap();
    let bridge = format!("{}/examples/fixture_bridge.py", env!("CARGO_MANIFEST_DIR"));
    let command = CommandSpec {
        program: bridge,
        args: vec![],
    };
    let mut p = Profile::template("generic", id);
    p.product_brief = "Produce an observable fixture result".into();
    p.runtime.command = command.clone();
    p.runtime.planner_model = "fixture-planner".into();
    p.review.command = Some(command.clone());
    p.review.reviewers = vec!["fixture-human".into()];
    p.checks = vec![Check {
        id: "result".into(),
        category: "delivered_behaviour".into(),
        required: true,
        command,
        criterion_ids: vec!["C1".into()],
        timeout_seconds: 10,
        resource: None,
    }];
    apply(&preview(dir.path(), &p).unwrap()).unwrap();
    (dir, p)
}
fn proposal() -> Proposal {
    Proposal {
        revision: "proposal-v1".into(),
        title: "Observable result".into(),
        scope: "Write result.txt".into(),
        criteria: vec![Criterion {
            id: "C1".into(),
            description: "Persist verified result".into(),
            required: true,
            check_ids: vec!["result".into()],
        }],
        journeys: vec![Journey {
            id: "J1".into(),
            description: "Read result after write".into(),
            criterion_ids: vec!["C1".into()],
            check_ids: vec!["result".into()],
            required: true,
        }],
        required_checks: vec!["result".into()],
    }
}
fn approve(root: &Path, run: &Run, action: DecisionAction, rev: &str) -> DecisionClaim {
    let claim = DecisionClaim {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: run.project_id.clone(),
        run_id: run.id.clone(),
        actor: "fixture-human".into(),
        decided_at: "2026-09-08T00:00:00Z".into(),
        artifact_revision: rev.into(),
        source: "fixture://receipt".into(),
        action,
    };
    let dir = root.join(".product-workflow/fixture-decisions");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{}.json", claim.id)),
        serde_json::to_vec(&claim).unwrap(),
    )
    .unwrap();
    claim
}
#[test]
fn complete_pipeline_repairs_and_preserves_authority() {
    let (d, _) = project("alpha");
    fs::write(d.path().join(".fixture-fail-first"), "yes").unwrap();
    let run = adapters::new_run(d.path(), proposal()).unwrap();
    assert!(adapters::advance(d.path(), &run.id).is_err());
    let build = approve(d.path(), &run, DecisionAction::Build, "proposal-v1");
    adapters::decision(d.path(), &run.id, build).unwrap();
    let mut current = run;
    for _ in 0..8 {
        current = adapters::advance(d.path(), &current.id).unwrap();
    }
    assert_eq!(current.stage, Stage::AwaitingAcceptance);
    assert_eq!(current.iteration, 2);
    let accept = approve(
        d.path(),
        &current,
        DecisionAction::Accept,
        current.candidate_revision().unwrap(),
    );
    let accepted = adapters::decision(d.path(), &current.id, accept).unwrap();
    assert_eq!(accepted.stage, Stage::Accepted);
    assert!(!accepted.has_authority(
        DecisionAction::Merge,
        accepted.candidate_revision().unwrap()
    ));
}
#[test]
fn setup_repeat_manual_conflict_detach_rollback_and_isolation() {
    let (a, p) = project("alpha");
    let (b, _) = project("beta");
    assert!(preview(a.path(), &p).unwrap().changes.is_empty());
    let b_lock = load_lock(b.path()).unwrap();
    let mut changed = p.clone();
    changed.name = "New name".into();
    let update = preview(a.path(), &changed).unwrap();
    apply(&update).unwrap();
    assert_eq!(load_lock(b.path()).unwrap(), b_lock);
    apply(&rollback_preview(a.path(), &update.id).unwrap()).unwrap();
    assert_eq!(load_profile(a.path()).unwrap(), p);
    let history = a.path().join(".product-workflow/runs/history.txt");
    fs::create_dir_all(history.parent().unwrap()).unwrap();
    fs::write(&history, "retain").unwrap();
    let detach = detach_preview(a.path()).unwrap();
    apply(&detach).unwrap();
    assert!(history.exists());
    assert_eq!(
        fs::read_to_string(a.path().join("AGENTS.md")).unwrap(),
        "Preserve other files.\n"
    );
    apply(&rollback_preview(a.path(), &detach.id).unwrap()).unwrap();
    fs::write(
        a.path().join(".product-workflow/profile.json"),
        "manual edit",
    )
    .unwrap();
    assert!(preview(a.path(), &p).is_err());
    assert!(detach_preview(a.path()).is_err());
}
#[test]
fn stale_preview_is_rejected() {
    let (d, p) = project("alpha");
    let mut p2 = p;
    p2.name = "changed".into();
    let plan = preview(d.path(), &p2).unwrap();
    fs::write(d.path().join(".product-workflow/context.json"), "manual").unwrap();
    assert!(apply(&plan).is_err());
}
#[test]
fn interrupted_setup_completes_or_preserves_later_edits() {
    let (d, p) = project("alpha");
    let mut p2 = p;
    p2.name = "changed".into();
    let plan = preview(d.path(), &p2).unwrap();
    let dir = d.path().join(".product-workflow/transactions");
    let journal = dir.join(format!("{}.json", plan.id));
    fs::write(
        &journal,
        serde_json::to_vec(&json!({"plan":plan,"state":"pending"})).unwrap(),
    )
    .unwrap();
    let first = &plan.changes[0];
    fs::write(d.path().join(&first.path), first.after.as_ref().unwrap()).unwrap();
    assert!(load_profile(d.path()).is_err());
    assert_eq!(recover(d.path(), false).unwrap(), 1);
    assert_eq!(load_profile(d.path()).unwrap(), p2);
}
#[test]
fn changed_candidate_and_cross_project_approval_are_rejected() {
    let (a, _) = project("alpha");
    let (b, _) = project("beta");
    let run = adapters::new_run(a.path(), proposal()).unwrap();
    let other = adapters::new_run(b.path(), proposal()).unwrap();
    let claim = approve(a.path(), &run, DecisionAction::Build, "proposal-v1");
    assert!(adapters::decision(b.path(), &other.id, claim.clone()).is_err());
    adapters::decision(a.path(), &run.id, claim).unwrap();
    for _ in 0..4 {
        adapters::advance(a.path(), &run.id).unwrap();
    }
    let current = adapters::load_run(a.path(), &run.id).unwrap();
    fs::write(a.path().join("result.txt"), "changed after review").unwrap();
    let accept = approve(
        a.path(),
        &current,
        DecisionAction::Accept,
        current.candidate_revision().unwrap(),
    );
    assert!(adapters::decision(a.path(), &run.id, accept).is_err());
}
#[test]
fn profile_update_does_not_change_active_snapshot() {
    let (d, p) = project("alpha");
    let run = adapters::new_run(d.path(), proposal()).unwrap();
    let mut updated = p;
    updated.name = "changed".into();
    apply(&preview(d.path(), &updated).unwrap()).unwrap();
    assert_ne!(
        adapters::pinned_profile(&adapters::load_run(d.path(), &run.id).unwrap())
            .unwrap()
            .name,
        updated.name
    );
}
#[cfg(unix)]
#[test]
fn symlink_setup_escape_is_rejected() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), root.path().join(".product-workflow")).unwrap();
    assert!(preview(root.path(), &Profile::template("generic", "safe")).is_err());
    assert!(!outside.path().join("profile.json").exists());
}
#[test]
fn command_arguments_are_not_shell_code() {
    let d = tempfile::tempdir().unwrap();
    let cmd = CommandSpec {
        program: "/usr/bin/printf".into(),
        args: vec![
            "%s".into(),
            "{\"text\":\"$(touch should-not-exist)\"}".into(),
        ],
    };
    let v = adapters::invoke(&cmd, d.path(), &json!({}), 3).unwrap();
    assert_eq!(v["text"], "$(touch should-not-exist)");
    assert!(!d.path().join("should-not-exist").exists());
}

#[test]
fn deleted_evidence_index_and_adapter_changes_block_authority() {
    let (d, _) = project("evidence");
    let run = adapters::new_run(d.path(), proposal()).unwrap();
    let claim = approve(d.path(), &run, DecisionAction::Build, "proposal-v1");
    adapters::decision(d.path(), &run.id, claim).unwrap();
    for _ in 0..4 {
        adapters::advance(d.path(), &run.id).unwrap();
    }
    let current = adapters::load_run(d.path(), &run.id).unwrap();
    fs::remove_file(
        adapters::run_dir(d.path(), &run.id)
            .unwrap()
            .join("evidence-1.json"),
    )
    .unwrap();
    let accept = approve(
        d.path(),
        &current,
        DecisionAction::Accept,
        current.candidate_revision().unwrap(),
    );
    assert!(
        adapters::decision(d.path(), &run.id, accept)
            .unwrap_err()
            .to_string()
            .contains("index")
    );
}
#[test]
fn source_drift_blocks_before_any_dispatch() {
    let (d, _) = project("drift");
    let run = adapters::new_run(d.path(), proposal()).unwrap();
    let claim = approve(d.path(), &run, DecisionAction::Build, "proposal-v1");
    adapters::decision(d.path(), &run.id, claim).unwrap();
    fs::write(d.path().join("unexpected.txt"), "unapproved").unwrap();
    assert!(adapters::advance(d.path(), &run.id).is_err());
    assert!(
        adapters::load_run(d.path(), &run.id)
            .unwrap()
            .active_dispatch
            .is_none()
    );
}
#[test]
fn interrupted_completed_role_reconciles_without_a_second_job() {
    let (d, _) = project("recovery");
    let run = adapters::new_run(d.path(), proposal()).unwrap();
    let claim = approve(d.path(), &run, DecisionAction::Build, "proposal-v1");
    adapters::decision(d.path(), &run.id, claim).unwrap();
    let mut planned = adapters::advance(d.path(), &run.id).unwrap();
    let dispatch = planned.current().dispatches[0].clone();
    planned.stage = Stage::Planning;
    planned.active_dispatch = Some(dispatch);
    adapters::save_run(d.path(), &planned).unwrap();
    let recovered = adapters::advance(d.path(), &run.id).unwrap();
    assert_eq!(recovered.stage, Stage::Implementing);
    assert_eq!(
        fs::read_dir(d.path().join(".product-workflow/fixture-jobs"))
            .unwrap()
            .count(),
        1
    );
}
#[test]
fn native_check_lookup_reuses_the_original_result() {
    let d = tempfile::tempdir().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let bridge=CommandSpec{program:"python3".into(),args:vec![format!("{}/bridges/native_check.py",env!("CARGO_MANIFEST_DIR")),"--".into(),"python3".into(),"-c".into(),"from pathlib import Path; p=Path('calls'); p.write_text(p.read_text()+'x' if p.exists() else 'x')".into()]};
    let request = json!({"operation":"check","protocol_version":1,"run_id":id,"idempotency_key":"check-1","check_id":"native","revision":"v1","category":"delivered_behaviour"});
    assert_eq!(
        adapters::invoke(&bridge, d.path(), &request, 5).unwrap()["outcome"],
        "passed"
    );
    let mut lookup = request;
    lookup["operation"] = json!("lookup_check");
    assert_eq!(
        adapters::invoke(&bridge, d.path(), &lookup, 5).unwrap()["outcome"],
        "passed"
    );
    assert_eq!(fs::read_to_string(d.path().join("calls")).unwrap(), "x");
}

#[test]
fn discovery_keeps_three_separate_passes_and_never_grants_build_authority() {
    let (d, _) = project("discovery");
    let record =
        adapters::discovery_start(d.path(), "What evidence supports an improvement?").unwrap();
    let id = record["id"].as_str().unwrap();
    for _ in 0..2 {
        assert_eq!(
            adapters::discovery_step(d.path(), id).unwrap()["status"],
            "ready"
        );
    }
    let result = adapters::discovery_step(d.path(), id).unwrap();
    assert_eq!(result["status"], "ready_for_product_review");
    assert_eq!(result["outputs"].as_array().unwrap().len(), 3);
    assert!(!d.path().join(".product-workflow/runs").exists());
    assert!(adapters::discovery_step(d.path(), id).is_err());
}
#[test]
fn installed_release_and_alternate_project_check_complete_without_core_changes() {
    let install = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_softwarefactory");
    let status = std::process::Command::new(binary)
        .args(["install", "--prefix"])
        .arg(install.path())
        .arg("--apply")
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(
        install
            .path()
            .join("releases/0.1.0/bridges/native_check.py")
            .exists()
    );
    let version = std::process::Command::new(install.path().join("bin/softwarefactory"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    let (d, mut p) = project("second-project");
    p.checks[0].id = "stored-outcome".into();
    p.checks[0].criterion_ids = vec!["SECOND".into()];
    p.checks[0].command = CommandSpec {
        program: "python3".into(),
        args: vec![
            install
                .path()
                .join("releases/0.1.0/bridges/native_check.py")
                .display()
                .to_string(),
            "--".into(),
            "python3".into(),
            "-c".into(),
            "from pathlib import Path; assert Path('result.txt').read_text()=='verified\\n'".into(),
        ],
    };
    apply(&preview(d.path(), &p).unwrap()).unwrap();
    let mut specification = proposal();
    specification.criteria[0].id = "SECOND".into();
    specification.criteria[0].check_ids = vec!["stored-outcome".into()];
    specification.journeys[0].criterion_ids = vec!["SECOND".into()];
    specification.journeys[0].check_ids = vec!["stored-outcome".into()];
    specification.required_checks = vec!["stored-outcome".into()];
    let run = adapters::new_run(d.path(), specification).unwrap();
    let claim = approve(d.path(), &run, DecisionAction::Build, "proposal-v1");
    adapters::decision(d.path(), &run.id, claim).unwrap();
    let mut current = run;
    for _ in 0..4 {
        current = adapters::advance(d.path(), &current.id).unwrap();
    }
    assert_eq!(current.stage, Stage::AwaitingAcceptance);
}
