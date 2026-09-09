use serde_json::json;
use softwarefactory::workflow::{self, Workflow, WorkflowState};
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

fn reach(root: &Path, id: &str, wanted: WorkflowState) -> Workflow {
    for _ in 0..35 {
        let state = workflow::advance(root, id).unwrap();
        assert_ne!(
            state.state,
            WorkflowState::Blocked,
            "{:?}",
            state.blocked_reason
        );
        if state.state == wanted {
            return state;
        }
    }
    panic!(
        "did not reach {wanted:?}: {:?}",
        workflow::load(root, id).unwrap()
    );
}
fn approve_current(root: &Path, state: &Workflow, action: DecisionAction) {
    let run = adapters::load_run(root, state.current_cycle().run_id.as_ref().unwrap()).unwrap();
    let rev = if action == DecisionAction::Build {
        &run.proposal.revision
    } else {
        run.candidate_revision().unwrap()
    };
    approve(root, &run, action, rev);
    adapters::poll_feature_decisions(root, &run.id).unwrap();
}
#[test]
fn two_complete_cycles_keep_distinct_human_gates_and_prior_learning() {
    let (d, _) = project("full-loop");
    let root = d.path();
    fs::write(root.join(".fixture-propose"), "yes").unwrap();
    fs::write(root.join(".fixture-fail-first"), "yes").unwrap();
    let initial = workflow::start(root, "Improve the product", Some(2)).unwrap();
    for cycle in 1..=2 {
        let gate = reach(root, &initial.id, WorkflowState::AwaitingBuildApproval);
        assert_eq!(gate.current_cycle().number, cycle);
        let run_id = gate.current_cycle().run_id.clone().unwrap();
        assert_eq!(
            adapters::load_run(root, &run_id).unwrap().dispatches_used,
            0
        );
        assert_eq!(
            workflow::advance(root, &initial.id).unwrap().state,
            WorkflowState::AwaitingBuildApproval
        );
        approve_current(root, &gate, DecisionAction::Build);
        let acceptance = reach(root, &initial.id, WorkflowState::AwaitingAcceptance);
        assert_eq!(adapters::load_run(root, &run_id).unwrap().iteration, 2);
        assert_eq!(
            workflow::advance(root, &initial.id).unwrap().cycles.len(),
            cycle as usize
        );
        approve_current(root, &acceptance, DecisionAction::Accept);
        let done = reach(root, &initial.id, WorkflowState::CycleComplete);
        assert!(done.current_cycle().retrospective_summary.is_some());
    }
    let done = reach(root, &initial.id, WorkflowState::Completed);
    assert_ne!(done.cycles[0].run_id, done.cycles[1].run_id);
    assert!(done.dispatches_used >= 20);
    let discovery = adapters::discovery_load(root, &done.cycles[1].discovery_id).unwrap();
    assert_eq!(
        discovery["workflow_context"]["prior_cycles"][0]["run_id"],
        json!(done.cycles[0].run_id)
    );
    assert_eq!(
        discovery["workflow_context"]["prior_cycles"][0]["retrospective_summary"],
        json!(done.cycles[0].retrospective_summary)
    );
    let accepted =
        adapters::verified_acceptance(root, done.current_cycle().run_id.as_ref().unwrap()).unwrap();
    assert!(!accepted.has_authority(
        DecisionAction::Merge,
        accepted.candidate_revision().unwrap()
    ));
}
#[test]
fn complete_cycle_waits_for_verified_harness_adoption() {
    let (d, _) = project("learning-loop");
    let root = d.path();
    fs::write(root.join(".fixture-propose"), "yes").unwrap();
    fs::write(root.join(".fixture-harness-candidate"), "yes").unwrap();
    let initial = workflow::start(root, "Improve the product", Some(1)).unwrap();
    let gate = reach(root, &initial.id, WorkflowState::AwaitingBuildApproval);
    approve_current(root, &gate, DecisionAction::Build);
    let gate = reach(root, &initial.id, WorkflowState::AwaitingAcceptance);
    approve_current(root, &gate, DecisionAction::Accept);
    let gate = reach(root, &initial.id, WorkflowState::AwaitingAdoption);
    let candidate_id = gate.current_cycle().learning_id.as_ref().unwrap();
    let candidate = softwarefactory::learning::read(root, candidate_id).unwrap();
    assert_eq!(candidate["disposition"], "candidate");
    assert_eq!(
        workflow::advance(root, &initial.id).unwrap().state,
        WorkflowState::AwaitingAdoption
    );
    let run = adapters::load_run(root, gate.current_cycle().run_id.as_ref().unwrap()).unwrap();
    approve(
        root,
        &run,
        DecisionAction::HarnessAdopt,
        candidate["revision"].as_str().unwrap(),
    );
    adapters::sync_harness_review(root, candidate_id).unwrap();
    adapters::poll_harness_decisions(root, candidate_id).unwrap();
    reach(root, &initial.id, WorkflowState::Completed);
}
#[test]
fn no_action_waits_without_repeated_spend_and_resume_keeps_history() {
    let (d, _) = project("no-action");
    let root = d.path();
    let initial = workflow::start(root, "Find opportunities", None).unwrap();
    let waiting = reach(root, &initial.id, WorkflowState::WaitingForOpportunity);
    assert_eq!(waiting.dispatches_used, 4);
    for _ in 0..3 {
        let state = workflow::advance(root, &initial.id).unwrap();
        assert_eq!(state.dispatches_used, 4);
        assert_eq!(state.cycles.len(), 1);
    }
    let resumed = workflow::resume(root, &initial.id).unwrap();
    assert_eq!(resumed.cycles.len(), 2);
    assert_ne!(
        resumed.cycles[0].discovery_id,
        resumed.cycles[1].discovery_id
    );
    assert_eq!(
        workflow::stop(root, &initial.id, "test complete")
            .unwrap()
            .state,
        WorkflowState::Stopped
    );
}
#[test]
fn replaying_child_creation_after_outer_save_interruption_keeps_one_child() {
    let (d, _) = project("recovery");
    let root = d.path();
    fs::write(root.join(".fixture-propose"), "yes").unwrap();
    let initial = workflow::start(root, "Improve", Some(1)).unwrap();
    let mut gate = reach(root, &initial.id, WorkflowState::AwaitingBuildApproval);
    let child = gate.current_cycle().run_id.clone();
    gate.state = WorkflowState::Discovery;
    fs::write(
        root.join(format!(
            ".product-workflow/workflows/{}/workflow.json",
            initial.id
        )),
        serde_json::to_vec(&gate).unwrap(),
    )
    .unwrap();
    let restored = workflow::advance(root, &initial.id).unwrap();
    assert_eq!(restored.current_cycle().run_id, child);
    assert_eq!(
        fs::read_dir(root.join(".product-workflow/runs"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn cli_runs_whole_pipeline_until_human_wait_and_can_continue_after_approval() {
    let (d, _) = project("cli-loop");
    let root = d.path();
    fs::write(root.join(".fixture-propose"), "yes").unwrap();
    let initial = workflow::start(root, "Improve", Some(1)).unwrap();
    let command = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_softwarefactory"))
            .arg("--project")
            .arg(root)
            .args(["workflow", "run", &initial.id, "--until-wait", "--review"])
            .output()
            .unwrap()
    };
    let output = command();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let gate = workflow::load(root, &initial.id).unwrap();
    assert_eq!(gate.state, WorkflowState::AwaitingBuildApproval);
    approve_current(root, &gate, DecisionAction::Build);
    let output = command();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let gate = workflow::load(root, &initial.id).unwrap();
    assert_eq!(gate.state, WorkflowState::AwaitingAcceptance);
    approve_current(root, &gate, DecisionAction::Accept);
    let output = command();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        workflow::load(root, &initial.id).unwrap().state,
        WorkflowState::Completed
    );
}

#[test]
fn elapsed_outer_budget_interrupts_fresh_work_but_allows_lookup() {
    let (d, _) = project("deadline");
    let root = d.path();
    let mut initial = workflow::start(root, "Improve", None).unwrap();
    initial.created_at = timestamp();
    initial.max_seconds = 1;
    fs::write(
        root.join(format!(
            ".product-workflow/workflows/{}/workflow.json",
            initial.id
        )),
        serde_json::to_vec(&initial).unwrap(),
    )
    .unwrap();
    let script = root.join("slow.py");
    fs::write(&script,"import time,json,sys\nr=json.load(sys.stdin)\nif r['operation']=='dispatch': time.sleep(10)\nprint('{}')\n").unwrap();
    let command = CommandSpec {
        program: "python3".into(),
        args: vec![script.to_string_lossy().into_owned()],
    };
    let start = std::time::Instant::now();
    assert!(
        adapters::invoke(
            &command,
            root,
            &json!({"operation":"dispatch","workflow_id":initial.id}),
            10
        )
        .is_err()
    );
    assert!(start.elapsed().as_secs() < 4);
    assert!(
        adapters::invoke(
            &command,
            root,
            &json!({"operation":"lookup","workflow_id":initial.id}),
            10
        )
        .is_ok()
    );
    assert!(
        adapters::invoke(
            &command,
            root,
            &json!({"operation":"check","workflow_id":initial.id}),
            10
        )
        .is_err()
    );
}
#[test]
fn deferred_harness_candidate_finishes_cycle_without_changing_configuration() {
    let (d, _) = project("deferred-loop");
    let root = d.path();
    fs::write(root.join(".fixture-propose"), "yes").unwrap();
    fs::write(root.join(".fixture-harness-candidate"), "yes").unwrap();
    let before = load_lock(root).unwrap();
    let initial = workflow::start(root, "Improve the product", Some(1)).unwrap();
    let gate = reach(root, &initial.id, WorkflowState::AwaitingBuildApproval);
    approve_current(root, &gate, DecisionAction::Build);
    let gate = reach(root, &initial.id, WorkflowState::AwaitingAcceptance);
    approve_current(root, &gate, DecisionAction::Accept);
    let gate = reach(root, &initial.id, WorkflowState::AwaitingAdoption);
    let candidate_id = gate.current_cycle().learning_id.as_ref().unwrap();
    let candidate = softwarefactory::learning::read(root, candidate_id).unwrap();
    assert_eq!(candidate["disposition"], "candidate");
    assert_eq!(
        workflow::advance(root, &initial.id).unwrap().state,
        WorkflowState::AwaitingAdoption
    );
    let run = adapters::load_run(root, gate.current_cycle().run_id.as_ref().unwrap()).unwrap();
    approve(
        root,
        &run,
        DecisionAction::HarnessDefer,
        candidate["revision"].as_str().unwrap(),
    );
    adapters::sync_harness_review(root, candidate_id).unwrap();
    adapters::poll_harness_decisions(root, candidate_id).unwrap();
    reach(root, &initial.id, WorkflowState::Completed);
    assert_eq!(load_lock(root).unwrap(), before);
    assert!(adapters::verified_adoption(root, candidate_id).is_err());
    assert_eq!(
        adapters::verified_harness_disposition(root, candidate_id).unwrap()["disposition"],
        "adoption_deferred"
    );
}
