use serde_json::{Value, json};
use softwarefactory::{
    adapters,
    engine::{DecisionAction, DecisionClaim, Stage},
    issues,
    setup::{self, Check, CommandSpec, Profile},
    workflow::{self, Workflow, WorkflowState},
};
use std::{fs, path::Path, process::Command};

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, value) in [
        (".softwarefactory-synthetic-fixture", "test only"),
        (".fixture-propose", "yes"),
        (".fixture-regression-first", "yes"),
        ("existing.txt", "preserved\n"),
    ] {
        fs::write(dir.path().join(name), value).unwrap();
    }
    let command = CommandSpec {
        program: format!("{}/examples/fixture_bridge.py", env!("CARGO_MANIFEST_DIR")),
        args: vec![],
    };
    let mut profile = Profile::template("generic", "issue-test");
    profile.product_brief = "Fix saving while preserving existing content".into();
    profile.runtime.command = command.clone();
    profile.runtime.planner_model = "fixture-planner".into();
    profile.review.command = Some(command.clone());
    profile.review.reviewers = vec!["fixture-human".into()];
    profile.checks = [("result", "ISSUE-FIX"), ("regression", "ISSUE-REGRESSION")]
        .into_iter()
        .map(|(id, criterion)| Check {
            id: id.into(),
            category: "delivered_behaviour".into(),
            required: true,
            command: command.clone(),
            criterion_ids: vec![criterion.into()],
            timeout_seconds: 10,
            resource: None,
        })
        .collect();
    setup::apply(&setup::preview(dir.path(), &profile).unwrap()).unwrap();
    dir
}
fn item(number: u64) -> Value {
    json!({"number":number,"title":"Save result","body":"Save must persist verified; preserve existing content.","html_url":format!("https://github.com/owner/repo/issues/{number}"),"state":"open","updated_at":"2026-09-11T12:00:00Z","labels":[{"name":"bug"}]})
}
/// Exercise the real CLI's gh argument contract with a deterministic executable.
fn scan(root: &Path, pages: Value) {
    let output = issue_cli(
        root,
        pages,
        &["scan", "--repo", "owner/repo", "--label", "bug"],
    );
    serde_json::from_slice::<Value>(&output).unwrap();
}
fn issue_cli(root: &Path, pages: Value, arguments: &[&str]) -> Vec<u8> {
    let bridge = tempfile::tempdir().unwrap();
    let file = bridge.path().join("gh");
    fs::write(&file, format!("#!/usr/bin/env python3\nimport sys,json\nassert sys.argv[1:]==['api','--hostname','github.com','--method','GET','repos/owner/repo/issues?state=open&per_page=100&sort=created&direction=asc','--paginate','--slurp'],sys.argv\nprint({})\n", serde_json::to_string(&serde_json::to_string(&pages).unwrap()).unwrap())).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_softwarefactory"))
        .args(["--project", root.to_str().unwrap(), "issues"])
        .args(arguments)
        .env(
            "PATH",
            format!(
                "{}:{}",
                bridge.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn reach(root: &Path, state: WorkflowState) -> Workflow {
    for _ in 0..40 {
        let child = issues::advance(root).unwrap().expect("active issue");
        assert_ne!(
            child.state,
            WorkflowState::Blocked,
            "{:?}",
            child.blocked_reason
        );
        if child.state == state {
            return child;
        }
    }
    panic!("did not reach {state:?}");
}
fn approve(root: &Path, child: &Workflow, action: DecisionAction) {
    let run = adapters::load_run(root, child.current_cycle().run_id.as_ref().unwrap()).unwrap();
    let revision = if action == DecisionAction::Build {
        run.proposal.revision.clone()
    } else {
        run.candidate_revision().unwrap().into()
    };
    let claim = DecisionClaim {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: run.project_id.clone(),
        run_id: run.id.clone(),
        actor: "fixture-human".into(),
        decided_at: "2026-09-11T12:00:00Z".into(),
        artifact_revision: revision,
        source: "fixture://receipt".into(),
        action,
    };
    let directory = root.join(".product-workflow/fixture-decisions");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join(format!("{}.json", claim.id)),
        serde_json::to_vec(&claim).unwrap(),
    )
    .unwrap();
    adapters::decision(root, &run.id, claim).unwrap();
}
#[test]
fn issue_fix_passes_but_regression_failure_forces_repair_and_fresh_checks() {
    let dir = project();
    let root = dir.path();
    scan(root, json!([[item(1)], [item(2)]]));
    let gate = reach(root, WorkflowState::AwaitingBuildApproval);
    let run_id = gate.current_cycle().run_id.as_ref().unwrap();
    let run = adapters::load_run(root, run_id).unwrap();
    assert_eq!(run.dispatches_used, 0);
    assert!(
        run.proposal
            .scope
            .contains("https://github.com/owner/repo/issues/1")
    );
    assert_eq!(
        issues::advance(root).unwrap().unwrap().state,
        WorkflowState::AwaitingBuildApproval
    );
    approve(root, &gate, DecisionAction::Build);
    let acceptance = reach(root, WorkflowState::AwaitingAcceptance);
    let run = adapters::load_run(root, run_id).unwrap();
    assert_eq!(run.iteration, 2, "regression must return to planner");
    let first = &run.iterations[0];
    assert!(first.checks.iter().any(|c| c.id == "result" && c.outcome == softwarefactory::engine::CheckOutcome::Passed));
    assert!(first.checks.iter().any(
        |c| c.id == "regression" && c.outcome == softwarefactory::engine::CheckOutcome::Failed
    ));
    assert!(
        run.current()
            .checks
            .iter()
            .all(|c| c.outcome == softwarefactory::engine::CheckOutcome::Passed)
    );
    assert_eq!(
        fs::read_to_string(root.join("existing.txt")).unwrap(),
        "preserved\n"
    );
    assert_eq!(
        issues::advance(root).unwrap().unwrap().state,
        WorkflowState::AwaitingAcceptance
    );
    approve(root, &acceptance, DecisionAction::Accept);
    reach(root, WorkflowState::Completed);
    assert_eq!(issues::status(root).unwrap().tasks[0].state, "accepted");
    scan(root, json!([[item(1), item(2)]]));
    assert_eq!(issues::load(root).unwrap().tasks.len(), 2);
    let second = issues::advance(root).unwrap().unwrap();
    assert_ne!(second.id, gate.id);
    assert_eq!(second.github_issue.unwrap().number, 2);
}
#[test]
fn missing_fix_or_regression_contract_blocks_before_implementation() {
    let dir = project();
    let root = dir.path();
    fs::write(root.join(".fixture-missing-issue-contract"), "yes").unwrap();
    scan(root, json!([[item(1)]]));
    let mut blocked = None;
    for _ in 0..10 {
        let child = issues::advance(root).unwrap().unwrap();
        if child.state == WorkflowState::Blocked {
            blocked = Some(child);
            break;
        }
    }
    let child = blocked.expect("invalid PM output must block");
    assert!(child.blocked_reason.unwrap().contains("ISSUE-FIX"));
    assert!(child.cycles[0].run_id.is_none());
    assert!(!root.join("result.txt").exists());
}
#[test]
fn recovery_freezes_source_and_does_not_duplicate_workflow() {
    let dir = project();
    let root = dir.path();
    scan(root, json!([[item(1)]]));
    // Simulate a crash after the task's durable reservation, before child creation.
    let mut queue = issues::load(root).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    queue.tasks[0].workflow_id = Some(id.clone());
    queue.tasks[0].started_issue = Some(queue.tasks[0].issue.clone());
    fs::write(
        root.join(".product-workflow/issues/queue.json"),
        serde_json::to_vec(&queue).unwrap(),
    )
    .unwrap();
    let mut edited = item(1);
    edited["body"] = json!("Changed requirements");
    scan(root, json!([[edited]]));
    let child = issues::advance(root).unwrap().unwrap();
    assert_eq!(child.id, id);
    assert!(child.question.contains("preserve existing content"));
    assert!(!child.question.contains("Changed requirements"));
    assert!(issues::status(root).unwrap().tasks[0].source_changed);
    issues::advance(root).unwrap();
    assert_eq!(workflow::list(root).unwrap().len(), 1);
    assert!(issues::retry(root, 1).is_err());
    workflow::stop(root, &id, "Replan edited issue").unwrap();
    issues::retry(root, 1).unwrap();
    let fresh = issues::advance(root).unwrap().unwrap();
    assert_ne!(fresh.id, id);
    assert!(fresh.question.contains("Changed requirements"));
    assert_eq!(
        issues::load(root).unwrap().tasks[0].previous_workflows,
        vec![id]
    );
}
#[test]
fn no_action_and_closed_issue_are_not_reported_as_fixed() {
    let dir = project();
    let root = dir.path();
    fs::remove_file(root.join(".fixture-propose")).unwrap();
    scan(root, json!([[item(1), item(2)]]));
    reach(root, WorkflowState::Completed);
    assert_eq!(issues::status(root).unwrap().tasks[0].state, "unresolved");
    scan(root, json!([[]]));
    assert!(issues::advance(root).unwrap().is_none());
    assert_eq!(issues::status(root).unwrap().tasks[1].state, "not_eligible");
    assert!(issues::retry(root, 1).is_err());
}
#[test]
fn stale_candidate_cannot_be_accepted_or_complete_issue() {
    let dir = project();
    let root = dir.path();
    scan(root, json!([[item(1)]]));
    let gate = reach(root, WorkflowState::AwaitingBuildApproval);
    approve(root, &gate, DecisionAction::Build);
    let child = reach(root, WorkflowState::AwaitingAcceptance);
    fs::write(root.join("existing.txt"), "changed after review\n").unwrap();
    let run = adapters::load_run(root, child.current_cycle().run_id.as_ref().unwrap()).unwrap();
    assert_eq!(run.stage, Stage::AwaitingAcceptance);
    // Existing engine validates the candidate again while verifying acceptance.
    let claim = DecisionClaim {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: run.project_id.clone(),
        run_id: run.id.clone(),
        actor: "fixture-human".into(),
        decided_at: "2026-09-11T12:00:00Z".into(),
        artifact_revision: run.candidate_revision().unwrap().into(),
        source: "fixture://receipt".into(),
        action: DecisionAction::Accept,
    };
    let directory = root.join(".product-workflow/fixture-decisions");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join(format!("{}.json", claim.id)),
        serde_json::to_vec(&claim).unwrap(),
    )
    .unwrap();
    assert!(adapters::decision(root, &run.id, claim).is_err());
    assert_ne!(issues::status(root).unwrap().tasks[0].state, "accepted");
}

#[test]
fn foreground_loop_reaches_human_gates_and_exits_when_idle() {
    let dir = project();
    let root = dir.path();
    let arguments = [
        "run",
        "--repo",
        "owner/repo",
        "--label",
        "bug",
        "--review",
        "--until-wait",
    ];
    issue_cli(root, json!([[item(1)]]), &arguments);
    let queue = issues::load(root).unwrap();
    let id = queue.tasks[0].workflow_id.as_ref().unwrap();
    let gate = workflow::load(root, id).unwrap();
    assert_eq!(gate.state, WorkflowState::AwaitingBuildApproval);
    approve(root, &gate, DecisionAction::Build);
    issue_cli(root, json!([[item(1)]]), &arguments);
    let gate = workflow::load(root, id).unwrap();
    assert_eq!(gate.state, WorkflowState::AwaitingAcceptance);
    approve(root, &gate, DecisionAction::Accept);
    issue_cli(root, json!([[item(1)]]), &arguments);
    assert_eq!(issues::status(root).unwrap().tasks[0].state, "accepted");
    assert_eq!(workflow::list(root).unwrap().len(), 1);
}
