//! Durable coordinator for the complete discovery → feature → retrospective → next-cycle loop.
//! This outer lock is distinct from the project lock used by its bounded child operations.
use crate::{
    adapters,
    engine::{Run, Stage},
    learning,
    setup::{self, CONFIG},
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    Discovery,
    Feature,
    AwaitingBuildApproval,
    AwaitingAcceptance,
    Retrospective,
    AwaitingAdoption,
    CycleComplete,
    WaitingForOpportunity,
    Blocked,
    Stopped,
    Completed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cycle {
    pub number: u32,
    pub question: String,
    pub discovery_id: String,
    pub run_id: Option<String>,
    pub retrospective_id: String,
    pub learning_id: Option<String>,
    pub next_question: Option<String>,
    pub pm_disposition: Option<String>,
    pub retrospective_summary: Option<String>,
}
impl Cycle {
    fn new(number: u32, question: String) -> Self {
        Self {
            number,
            question,
            discovery_id: Uuid::new_v4().to_string(),
            run_id: None,
            retrospective_id: Uuid::new_v4().to_string(),
            learning_id: None,
            next_question: None,
            pm_disposition: None,
            retrospective_summary: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub schema_version: u32,
    pub core_version: String,
    pub repository_root: PathBuf,
    pub id: String,
    pub project_id: String,
    pub question: String,
    pub state: WorkflowState,
    pub cycles: Vec<Cycle>,
    pub max_cycles: Option<u32>,
    pub blocked_reason: Option<String>,
    pub resume_state: Option<WorkflowState>,
    pub created_at: u64,
    pub max_dispatches: u32,
    pub max_seconds: u64,
    pub dispatches_used: u64,
    pub stop_requested: bool,
    pub events: Vec<Value>,
}
impl Workflow {
    pub fn is_waiting(&self) -> bool {
        matches!(
            self.state,
            WorkflowState::AwaitingBuildApproval
                | WorkflowState::AwaitingAcceptance
                | WorkflowState::AwaitingAdoption
                | WorkflowState::WaitingForOpportunity
                | WorkflowState::Blocked
        )
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            WorkflowState::Stopped | WorkflowState::Completed
        )
    }
    pub fn current_cycle(&self) -> &Cycle {
        self.cycles.last().expect("validated workflow has a cycle")
    }
    fn current_mut(&mut self) -> &mut Cycle {
        self.cycles
            .last_mut()
            .expect("validated workflow has a cycle")
    }
    fn event(&mut self, message: impl Into<String>) {
        self.events
            .push(json!({"at":setup::timestamp(),"message":message.into()}));
    }
    fn block(&mut self, reason: impl Into<String>) {
        if self.state != WorkflowState::Blocked {
            self.resume_state = Some(self.state);
        }
        let reason = reason.into();
        self.event(format!("Blocked: {reason}"));
        self.blocked_reason = Some(reason);
        self.state = WorkflowState::Blocked;
    }
}
struct WorkflowLock(File);
impl WorkflowLock {
    fn acquire(root: &Path) -> Result<Self> {
        let dir = setup::safe_path(root, CONFIG)?;
        fs::create_dir_all(dir)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(setup::safe_path(root, &format!("{CONFIG}/.workflow-lock"))?)?;
        file.try_lock_exclusive().context(
            "Another transition owns the whole workflow; a stop request remains observable",
        )?;
        Ok(Self(file))
    }
}
impl Drop for WorkflowLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}
fn directory(root: &Path, id: &str) -> Result<PathBuf> {
    ensure!(Uuid::parse_str(id).is_ok(), "Invalid workflow ID");
    setup::safe_path(root, &format!("{CONFIG}/workflows/{id}"))
}
fn path(root: &Path, id: &str) -> Result<PathBuf> {
    directory(root, id)?;
    setup::safe_path(root, &format!("{CONFIG}/workflows/{id}/workflow.json"))
}
fn stop_path(root: &Path, id: &str) -> Result<PathBuf> {
    setup::safe_path(root, &format!("{CONFIG}/workflows/{id}/stop-request.json"))
}
fn save(root: &Path, workflow: &Workflow) -> Result<()> {
    setup::atomic(
        &path(root, &workflow.id)?,
        setup::json(workflow)?.as_bytes(),
    )
}
fn read_raw(root: &Path, id: &str) -> Result<Workflow> {
    let workflow: Workflow = serde_json::from_slice(&fs::read(path(root, id)?)?)?;
    ensure!(
        workflow.schema_version == 1 && workflow.id == id,
        "Workflow schema or identity mismatch"
    );
    ensure!(
        workflow.project_id == setup::load_profile(root)?.project_id,
        "Workflow belongs to another project"
    );
    ensure!(
        !workflow.cycles.is_empty()
            && workflow
                .cycles
                .iter()
                .enumerate()
                .all(|(i, c)| c.number as usize == i + 1),
        "Invalid cycle history"
    );
    ensure!(
        workflow.max_cycles.is_none_or(|limit| limit > 0)
            && workflow.max_dispatches > 0
            && workflow.max_seconds > 0,
        "Invalid workflow budgets"
    );
    for cycle in &workflow.cycles {
        ensure!(
            Uuid::parse_str(&cycle.discovery_id).is_ok()
                && Uuid::parse_str(&cycle.retrospective_id).is_ok(),
            "Invalid cycle identity"
        );
        if let Some(id) = &cycle.run_id {
            ensure!(Uuid::parse_str(id).is_ok(), "Invalid child feature ID");
        }
        if let Some(id) = &cycle.learning_id {
            ensure!(Uuid::parse_str(id).is_ok(), "Invalid learning ID");
        }
    }
    Ok(workflow)
}
fn feature_state(stage: Stage) -> WorkflowState {
    match stage {
        Stage::AwaitingBuildApproval => WorkflowState::AwaitingBuildApproval,
        Stage::AwaitingAcceptance => WorkflowState::AwaitingAcceptance,
        _ => WorkflowState::Feature,
    }
}
fn refresh_counts(root: &Path, workflow: &mut Workflow) -> Result<()> {
    let mut used = 0u64;
    for cycle in &workflow.cycles {
        let discovery_path = setup::safe_path(
            root,
            &format!("{CONFIG}/discovery/{}.json", cycle.discovery_id),
        )?;
        if discovery_path.is_file() {
            let discovery = adapters::discovery_load(root, &cycle.discovery_id)?;
            used += discovery["outputs"].as_array().map_or(0, |v| v.len()) as u64;
            if !discovery["pending"].is_null() {
                used += 1;
            }
        }
        if let Some(id) = &cycle.run_id {
            let child_path = setup::safe_path(root, &format!("{CONFIG}/runs/{id}/run.json"))?;
            if child_path.is_file() {
                used += adapters::load_run(root, id)?.dispatches_used as u64;
            }
        }
        let retrospective_path = setup::safe_path(
            root,
            &format!("{CONFIG}/retrospectives/{}.json", cycle.retrospective_id),
        )?;
        if retrospective_path.is_file() {
            let retrospective: Value = serde_json::from_slice(&fs::read(retrospective_path)?)?;
            if !retrospective["pending"].is_null()
                || !retrospective["result"].is_null()
                || !retrospective["output"].is_null()
                || retrospective["status"] == "completed"
            {
                used += 1;
            }
        }
    }
    workflow.dispatches_used = used;
    Ok(())
}
pub fn load(root: &Path, id: &str) -> Result<Workflow> {
    let mut workflow = read_raw(root, id)?;
    refresh_counts(root, &mut workflow)?;
    workflow.stop_requested = stop_path(root, id)?.exists();
    if matches!(
        workflow.state,
        WorkflowState::Feature
            | WorkflowState::AwaitingBuildApproval
            | WorkflowState::AwaitingAcceptance
    ) && let Some(id) = &workflow.current_cycle().run_id
    {
        workflow.state = feature_state(adapters::load_run(root, id)?.stage);
    }
    Ok(workflow)
}
pub fn list(root: &Path) -> Result<Vec<Workflow>> {
    let dir = setup::safe_path(root, &format!("{CONFIG}/workflows"))?;
    let mut workflows = vec![];
    if !dir.exists() {
        return Ok(workflows);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if path(root, &id)?.is_file() {
                workflows.push(load(root, &id)?);
            }
        }
    }
    workflows.sort_by_key(|w| (w.created_at, w.id.clone()));
    Ok(workflows)
}
pub fn start(root: &Path, question: &str, max_cycles: Option<u32>) -> Result<Workflow> {
    let _guard = WorkflowLock::acquire(root)?;
    ensure!(
        !question.trim().is_empty(),
        "Supply a focused product question"
    );
    ensure!(
        max_cycles.is_none_or(|limit| limit > 0),
        "Cycle limit must be positive"
    );
    ensure!(
        list(root)?.iter().all(Workflow::is_terminal),
        "Another whole workflow is active in this project"
    );
    let profile = checked_profile(root)?;
    let mut workflow = Workflow {
        schema_version: 1,
        core_version: setup::VERSION.into(),
        repository_root: fs::canonicalize(root)?,
        id: Uuid::new_v4().to_string(),
        project_id: profile.project_id,
        question: question.into(),
        state: WorkflowState::Discovery,
        cycles: vec![Cycle::new(1, question.into())],
        max_cycles,
        blocked_reason: None,
        resume_state: None,
        created_at: setup::timestamp(),
        max_dispatches: profile.max_dispatches,
        max_seconds: profile.max_seconds,
        dispatches_used: 0,
        stop_requested: false,
        events: vec![],
    };
    workflow.event(
        "Whole workflow started; each cycle requires its own feature and adoption decisions",
    );
    save(root, &workflow)?;
    Ok(workflow)
}
fn checked_profile(root: &Path) -> Result<setup::Profile> {
    let profile = setup::load_profile(root)?;
    let lock = setup::load_lock(root)?;
    ensure!(
        lock["core"] == setup::VERSION,
        "Project pins another core release; use that release or explicitly adopt an update before discovery"
    );
    ensure!(
        lock["profile_sha256"] == setup::digest(setup::json(&profile)?.as_bytes()),
        "Project profile changed outside reviewed setup; reconcile configuration before discovery"
    );
    Ok(profile)
}
fn check_execution_context(root: &Path, workflow: &Workflow) -> Result<()> {
    checked_profile(root)?;
    ensure!(
        workflow.core_version == setup::VERSION,
        "Whole workflow requires its pinned core release {}; use that release or explicitly migrate this workflow",
        workflow.core_version
    );
    ensure!(
        workflow.repository_root == fs::canonicalize(root)?,
        "Project root moved; whole workflow requires an explicit migration before execution"
    );
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BudgetUse {
    Reconciliation,
    Check,
    Agent,
}
fn check_budget(workflow: &Workflow, usage: BudgetUse) -> Result<()> {
    // Saved-job lookups remain available after expiry. Fresh checks use elapsed
    // time, while fresh role dispatches consume both aggregate limits.
    if usage == BudgetUse::Reconciliation {
        return Ok(());
    }
    ensure!(
        setup::timestamp().saturating_sub(workflow.created_at) < workflow.max_seconds,
        "Whole-workflow elapsed-time budget exhausted; new cycles do not reset it"
    );
    ensure!(
        usage != BudgetUse::Agent || workflow.dispatches_used < workflow.max_dispatches as u64,
        "Whole-workflow dispatch budget exhausted; new cycles do not reset it"
    );
    Ok(())
}
/// Classify the next operation from durable job intent, never from a polling attempt.
fn next_work_kind(root: &Path, workflow: &Workflow, state: WorkflowState) -> Result<BudgetUse> {
    use BudgetUse::{Agent, Check, Reconciliation};
    let cycle = workflow.current_cycle();
    match state {
        WorkflowState::Discovery => {
            let path = setup::safe_path(
                root,
                &format!("{CONFIG}/discovery/{}.json", cycle.discovery_id),
            )?;
            if !path.is_file() {
                return Ok(Agent);
            }
            let record = adapters::discovery_load(root, &cycle.discovery_id)?;
            Ok(
                if record["next_stage"].as_u64().unwrap_or(0) < 3 && record["pending"].is_null() {
                    Agent
                } else {
                    Reconciliation
                },
            )
        }
        WorkflowState::Feature
        | WorkflowState::AwaitingBuildApproval
        | WorkflowState::AwaitingAcceptance => {
            let id = cycle
                .run_id
                .as_deref()
                .context("Missing feature resume target")?;
            let run = adapters::load_run(root, id)?;
            if run.active_dispatch.is_some() {
                return Ok(Reconciliation);
            }
            let stage = if run.stage == Stage::Blocked {
                run.resume_stage.context("Missing child resume stage")?
            } else {
                run.stage
            };
            if matches!(
                stage,
                Stage::AwaitingBuildApproval
                    | Stage::AwaitingAcceptance
                    | Stage::Accepted
                    | Stage::Cancelled
            ) {
                return Ok(Reconciliation);
            }
            if stage != Stage::Checking {
                return Ok(Agent);
            }
            let profile = adapters::pinned_profile(&run)?;
            let revision = run
                .candidate_revision()
                .context("Missing candidate for checks")?;
            for check in &profile.checks {
                if run.current().checks.iter().any(|result| {
                    result.id == check.id
                        && result.revision == revision
                        && result.execution == crate::engine::ExecutionState::Completed
                        && matches!(
                            result.outcome,
                            crate::engine::CheckOutcome::Passed
                                | crate::engine::CheckOutcome::Failed
                        )
                }) {
                    continue;
                }
                let key = format!(
                    "{}-{}-{}",
                    run.id,
                    run.iteration,
                    setup::digest(check.id.as_bytes())
                );
                let directory = adapters::run_dir(root, id)?;
                // The adapter independently gates later fresh checks in this batch.
                return Ok(
                    if directory.join(format!("check-intent-{key}.json")).exists()
                        || directory.join(format!("check-{key}.json")).exists()
                    {
                        Reconciliation
                    } else {
                        Check
                    },
                );
            }
            Ok(Reconciliation)
        }
        WorkflowState::Retrospective => {
            let path = setup::safe_path(
                root,
                &format!("{CONFIG}/retrospectives/{}.json", cycle.retrospective_id),
            )?;
            if !path.is_file() {
                return Ok(Agent);
            }
            let record: Value = serde_json::from_slice(&fs::read(path)?)?;
            Ok(
                if record["pending"].is_null() && record["output"].is_null() {
                    Agent
                } else {
                    Reconciliation
                },
            )
        }
        WorkflowState::AwaitingAdoption | WorkflowState::Stopped | WorkflowState::Completed => {
            Ok(Reconciliation)
        }
        WorkflowState::CycleComplete => Ok(
            if !workflow
                .max_cycles
                .is_some_and(|limit| workflow.cycles.len() as u32 >= limit)
                && !cycle
                    .pm_disposition
                    .as_deref()
                    .is_some_and(|s| matches!(s, "no_action" | "defer"))
            {
                Agent
            } else {
                Reconciliation
            },
        ),
        WorkflowState::WaitingForOpportunity => Ok(Agent),
        WorkflowState::Blocked => {
            anyhow::bail!("A blocked state requires its recorded resume point")
        }
    }
}
fn next_cycle(workflow: &mut Workflow) -> Result<()> {
    if workflow
        .max_cycles
        .is_some_and(|limit| workflow.cycles.len() as u32 >= limit)
    {
        workflow.state = WorkflowState::Completed;
        workflow.event("Requested cycle limit reached; session completed without claiming the product goal is solved");
        return Ok(());
    }
    check_budget(workflow, BudgetUse::Agent)?;
    let question = workflow
        .current_cycle()
        .next_question
        .clone()
        .unwrap_or_else(|| workflow.question.clone());
    let number = workflow.cycles.len() as u32 + 1;
    workflow.cycles.push(Cycle::new(number, question));
    workflow.state = WorkflowState::Discovery;
    workflow.event(format!(
        "Cycle {number} allocated with fresh project-scoped records"
    ));
    Ok(())
}
fn prior_context(workflow: &Workflow) -> Value {
    let previous: Vec<_> = workflow.cycles.iter().take(workflow.cycles.len().saturating_sub(1)).map(|cycle| json!({
        "number":cycle.number,"question":cycle.question,"discovery_id":cycle.discovery_id,"run_id":cycle.run_id,
        "retrospective_id":cycle.retrospective_id,"learning_id":cycle.learning_id,"pm_disposition":cycle.pm_disposition,
        "retrospective_summary":cycle.retrospective_summary,"next_question":cycle.next_question
    })).collect();
    json!({"workflow_id":workflow.id,"project_id":workflow.project_id,"cycle":workflow.current_cycle().number,"prior_cycles":previous,"authority":"Prior approvals apply only to their original artifacts; every new proposal requires new approval"})
}
fn transition(root: &Path, workflow: &mut Workflow) -> Result<()> {
    match workflow.state {
        WorkflowState::Discovery => {
            let cycle = workflow.current_cycle().clone();
            let discovery_path = setup::safe_path(
                root,
                &format!("{CONFIG}/discovery/{}.json", cycle.discovery_id),
            )?;
            if !discovery_path.is_file() {
                check_budget(workflow, BudgetUse::Agent)?;
                adapters::discovery_start_with_id(
                    root,
                    &cycle.question,
                    &cycle.discovery_id,
                    prior_context(workflow),
                )?;
                return Ok(());
            }
            let discovery = adapters::discovery_load(root, &cycle.discovery_id)?;
            if discovery["next_stage"]
                .as_u64()
                .context("Invalid discovery stage")?
                < 3
            {
                check_budget(
                    workflow,
                    if discovery["pending"].is_null() {
                        BudgetUse::Agent
                    } else {
                        BudgetUse::Reconciliation
                    },
                )?;
                adapters::discovery_step(root, &cycle.discovery_id)?;
                return Ok(());
            }
            let pm = discovery["outputs"]
                .as_array()
                .and_then(|outputs| outputs.last())
                .context("Discovery has no PM output")?["output"]
                .clone();
            if let Some(proposal) = pm.get("proposal") {
                ensure!(
                    discovery["profile"] == serde_json::to_value(setup::load_profile(root)?)?,
                    "Project profile changed during discovery; restore the pinned profile before linking its feature"
                );
                let id = if let Some(id) = &cycle.run_id {
                    id.clone()
                } else {
                    let id = Uuid::new_v4().to_string();
                    workflow.current_mut().run_id = Some(id.clone());
                    save(root, workflow)?;
                    id
                };
                let run = adapters::new_run_with_id(
                    root,
                    serde_json::from_value(proposal.clone())?,
                    &id,
                )?;
                workflow.current_mut().pm_disposition = Some("proposal".into());
                workflow.state = feature_state(run.stage);
                workflow.event("Linked the PM proposal to a separately approved feature run");
            } else {
                let disposition = pm["disposition"]
                    .as_str()
                    .context("PM output has no proposal or disposition")?;
                ensure!(
                    ["no_action", "defer", "research"].contains(&disposition),
                    "Unsupported PM disposition"
                );
                workflow.current_mut().pm_disposition = Some(disposition.into());
                workflow.current_mut().next_question = pm["next_question"]
                    .as_str()
                    .or_else(|| pm["question"].as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_owned);
                if disposition == "research" {
                    ensure!(
                        workflow.current_cycle().next_question.is_some(),
                        "PM research disposition needs a focused follow-up question"
                    );
                    workflow.state = WorkflowState::CycleComplete;
                } else {
                    workflow.state = WorkflowState::Retrospective;
                }
                workflow.event(format!("PM disposition recorded: {disposition}"));
            }
        }
        WorkflowState::Feature
        | WorkflowState::AwaitingBuildApproval
        | WorkflowState::AwaitingAcceptance => {
            let id = workflow
                .current_cycle()
                .run_id
                .clone()
                .context("Feature stage has no linked run")?;
            let run = adapters::load_run(root, &id)?;
            match run.stage {
                Stage::AwaitingBuildApproval | Stage::AwaitingAcceptance => {
                    workflow.state = feature_state(run.stage);
                }
                Stage::Accepted => {
                    adapters::verified_acceptance(root, &id)?;
                    workflow.state = WorkflowState::Retrospective;
                    workflow.event("Human acceptance verified on the exact candidate; merge and release remain separate");
                }
                Stage::Cancelled => {
                    workflow.block("Linked feature was cancelled; stop this workflow or resolve the child disposition explicitly");
                }
                Stage::Blocked => {
                    workflow.block(format!(
                        "Linked feature is blocked: {}",
                        run.blocked_reason.as_deref().unwrap_or("inspect its run")
                    ));
                }
                _ => {
                    check_budget(
                        workflow,
                        next_work_kind(root, workflow, WorkflowState::Feature)?,
                    )?;
                    let run: Run = adapters::advance(root, &id)?;
                    workflow.state = feature_state(run.stage);
                    if run.stage == Stage::Blocked {
                        workflow.block(
                            run.blocked_reason
                                .unwrap_or_else(|| "Linked feature blocked".into()),
                        );
                    }
                }
            }
        }
        WorkflowState::Retrospective => {
            let cycle = workflow.current_cycle().clone();
            check_budget(
                workflow,
                next_work_kind(root, workflow, WorkflowState::Retrospective)?,
            )?;
            let output = adapters::retrospective(
                root,
                &workflow.id,
                cycle.number,
                &cycle.retrospective_id,
                &cycle.discovery_id,
                cycle.run_id.as_deref(),
            )?;
            let disposition = output["disposition"]
                .as_str()
                .context("Retrospective missing disposition")?;
            ensure!(
                ["no_change", "deferred", "candidate"].contains(&disposition),
                "Retrospective disposition is invalid"
            );
            let summary = output["summary"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .context("Retrospective needs an evidence-grounded summary")?;
            workflow.current_mut().retrospective_summary = Some(summary.into());
            if let Some(question) = output["next_question"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
            {
                workflow.current_mut().next_question = Some(question.into());
            }
            if disposition == "candidate" {
                let run_id = cycle.run_id.context(
                    "Harness adoption candidate requires a linked feature and its evidence",
                )?;
                let input = output
                    .get("learning_input")
                    .context("Retrospective candidate needs actual comparison evidence")?
                    .clone();
                let id = if let Some(id) = cycle.learning_id {
                    id
                } else {
                    let id = Uuid::new_v4().to_string();
                    workflow.current_mut().learning_id = Some(id.clone());
                    save(root, workflow)?;
                    id
                };
                learning::create_with_id(root, &run_id, input, &id)?;
                workflow.state = WorkflowState::AwaitingAdoption;
            } else {
                workflow.state = WorkflowState::CycleComplete;
            }
            workflow.event(format!("Retrospective recorded: {disposition}"));
        }
        WorkflowState::AwaitingAdoption => {
            let id = workflow
                .current_cycle()
                .learning_id
                .clone()
                .context("Missing harness candidate")?;
            let record = learning::read(root, &id)?;
            if record["disposition"] == "adoption_authorized"
                || record["disposition"] == "adoption_deferred"
            {
                let verified = adapters::verified_harness_disposition(root, &id)?;
                workflow.state = WorkflowState::CycleComplete;
                workflow.event(if verified["disposition"] == "adoption_authorized" {
                    "Exact harness adoption decision verified; any setup migration remains explicit"
                } else { "Human deferral verified for the exact harness candidate; existing setup retained" });
            }
        }
        WorkflowState::CycleComplete => {
            if workflow
                .max_cycles
                .is_some_and(|limit| workflow.cycles.len() as u32 >= limit)
            {
                next_cycle(workflow)?;
            } else if workflow
                .current_cycle()
                .pm_disposition
                .as_deref()
                .is_some_and(|s| matches!(s, "no_action" | "defer"))
            {
                workflow.state = WorkflowState::WaitingForOpportunity;
                workflow.event("No actionable opportunity; paused until an explicit resume rather than repeating paid discovery");
            } else {
                next_cycle(workflow)?;
            }
        }
        WorkflowState::WaitingForOpportunity
        | WorkflowState::Blocked
        | WorkflowState::Stopped
        | WorkflowState::Completed => {}
    }
    Ok(())
}
fn reconcile_stop(root: &Path, workflow: &mut Workflow) -> Result<()> {
    workflow.stop_requested = true;
    let cycle = workflow.current_cycle().clone();
    match adapters::cancel_workflow_job(
        root,
        &workflow.id,
        &cycle.discovery_id,
        &cycle.retrospective_id,
        cycle.run_id.as_deref(),
    ) {
        Ok(()) => {
            workflow.state = WorkflowState::Stopped;
            workflow.blocked_reason = None;
            workflow.resume_state = None;
            workflow
                .event("Stop confirmed; active child execution is terminated or already completed");
        }
        Err(error) => {
            workflow.block(format!(
                "Stop requested but child termination is unconfirmed: {error}"
            ));
        }
    }
    save(root, workflow)
}
pub fn advance(root: &Path, id: &str) -> Result<Workflow> {
    let _guard = WorkflowLock::acquire(root)?;
    let mut workflow = load(root, id)?;
    if workflow.is_terminal() {
        return Ok(workflow);
    }
    if workflow.stop_requested {
        reconcile_stop(root, &mut workflow)?;
        return Ok(workflow);
    }
    let result =
        check_execution_context(root, &workflow).and_then(|()| transition(root, &mut workflow));
    if let Err(error) = result {
        workflow.block(error.to_string());
    }
    refresh_counts(root, &mut workflow)?;
    if stop_path(root, id)?.exists() {
        reconcile_stop(root, &mut workflow)?;
    } else {
        save(root, &workflow)?;
    }
    Ok(workflow)
}
pub fn stop(root: &Path, id: &str, reason: &str) -> Result<Workflow> {
    ensure!(!reason.trim().is_empty(), "Supply a stop reason");
    let mut workflow = load(root, id)?;
    if workflow.is_terminal() {
        return Ok(workflow);
    }
    // This marker is deliberately written without taking either coordinator lock.
    setup::atomic(&stop_path(root, id)?, setup::json(&json!({"workflow_id":id,"project_id":workflow.project_id,"reason":reason,"requested_at":setup::timestamp()}))?.as_bytes())?;
    workflow.stop_requested = true;
    // Feature commands observe their existing per-run cancellation marker. Request it
    // before waiting for the outer lock, which the active feature transition owns.
    if let Some(run_id) = workflow.current_cycle().run_id.as_deref() {
        let run_path = setup::safe_path(root, &format!("{CONFIG}/runs/{run_id}/run.json"))?;
        if run_path.exists() {
            let run = adapters::load_run(root, run_id)?;
            if !matches!(run.stage, Stage::Accepted | Stage::Cancelled)
                && let Err(error) =
                    adapters::cancel(root, run_id, format!("Whole workflow stopped: {reason}"))
            {
                workflow.blocked_reason = Some(format!(
                    "Stop requested; child cancellation requires reconciliation: {error}"
                ));
            }
        }
    }
    match WorkflowLock::acquire(root) {
        Ok(_guard) => {
            workflow = load(root, id)?;
            reconcile_stop(root, &mut workflow)?;
        }
        Err(_) => {
            workflow.event(
                "Stop request saved; the active transition will reconcile child cancellation",
            );
        }
    }
    Ok(workflow)
}
pub fn resume(root: &Path, id: &str) -> Result<Workflow> {
    let _guard = WorkflowLock::acquire(root)?;
    let mut workflow = load(root, id)?;
    ensure!(
        !workflow.is_terminal(),
        "A stopped or completed workflow is terminal; start a new session explicitly"
    );
    ensure!(
        !workflow.stop_requested,
        "Stop is pending; reconcile cancellation before starting further work"
    );
    check_execution_context(root, &workflow)?;
    let target = if workflow.state == WorkflowState::Blocked {
        workflow
            .resume_state
            .context("Missing workflow resume point")?
    } else {
        workflow.state
    };
    check_budget(&workflow, next_work_kind(root, &workflow, target)?)?;
    match workflow.state {
        WorkflowState::WaitingForOpportunity => {
            next_cycle(&mut workflow)?;
        }
        WorkflowState::Blocked => {
            let resume = workflow
                .resume_state
                .context("Missing workflow resume point")?;
            if matches!(
                resume,
                WorkflowState::Feature
                    | WorkflowState::AwaitingBuildApproval
                    | WorkflowState::AwaitingAcceptance
            ) {
                let id = workflow
                    .current_cycle()
                    .run_id
                    .as_deref()
                    .context("Missing feature resume target")?;
                let child = adapters::load_run(root, id)?;
                ensure!(
                    child.stage != Stage::Cancelled,
                    "Cancelled feature cannot be silently restarted"
                );
                if child.stage == Stage::Blocked {
                    // The existing adapter performs its ordinary verified resume and exact-key reconciliation.
                    let child = adapters::advance(root, id)?;
                    if child.stage == Stage::Blocked {
                        workflow.block(
                            child
                                .blocked_reason
                                .unwrap_or_else(|| "Linked feature remains blocked".into()),
                        );
                        refresh_counts(root, &mut workflow)?;
                        save(root, &workflow)?;
                        return Ok(workflow);
                    }
                }
            }
            workflow.state = resume;
            workflow.resume_state = None;
            workflow.blocked_reason = None;
            workflow.event(
                "Workflow resumed from its recorded point with the original aggregate budgets",
            );
        }
        _ => {}
    }
    refresh_counts(root, &mut workflow)?;
    save(root, &workflow)?;
    Ok(workflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mut profile = setup::Profile::template("generic", "outer-loop-fixture");
        profile.product_brief = "Improve the fixture product through approved cycles".into();
        setup::apply(&setup::preview(dir.path(), &profile).unwrap()).unwrap();
        dir
    }
    #[test]
    fn continuous_start_preallocates_durable_ids_and_one_active_workflow() {
        let dir = project();
        let workflow = start(dir.path(), "Improve recovery", None).unwrap();
        assert_eq!(workflow.max_cycles, None);
        assert_eq!(workflow.state, WorkflowState::Discovery);
        assert!(Uuid::parse_str(&workflow.current_cycle().discovery_id).is_ok());
        assert!(Uuid::parse_str(&workflow.current_cycle().retrospective_id).is_ok());
        assert_eq!(
            load(dir.path(), &workflow.id)
                .unwrap()
                .current_cycle()
                .discovery_id,
            workflow.current_cycle().discovery_id
        );
        assert!(start(dir.path(), "Another active question", None).is_err());
    }
    #[test]
    fn no_action_pauses_and_only_explicit_resume_allocates_next_cycle() {
        let dir = project();
        let mut workflow = start(dir.path(), "Improve recovery", None).unwrap();
        workflow.state = WorkflowState::CycleComplete;
        workflow.current_mut().pm_disposition = Some("no_action".into());
        workflow.current_mut().next_question = Some("Inspect another scoped opportunity".into());
        save(dir.path(), &workflow).unwrap();
        let waiting = advance(dir.path(), &workflow.id).unwrap();
        assert_eq!(waiting.state, WorkflowState::WaitingForOpportunity);
        assert!(waiting.is_waiting());
        assert_eq!(advance(dir.path(), &workflow.id).unwrap().cycles.len(), 1);
        let resumed = resume(dir.path(), &workflow.id).unwrap();
        assert_eq!(resumed.cycles.len(), 2);
        assert_eq!(resumed.state, WorkflowState::Discovery);
        assert_eq!(
            resumed.current_cycle().question,
            "Inspect another scoped opportunity"
        );
        assert_ne!(
            resumed.cycles[0].discovery_id,
            resumed.cycles[1].discovery_id
        );
    }
    #[test]
    fn optional_cycle_cap_completes_session_without_new_dispatch() {
        let dir = project();
        let mut workflow = start(dir.path(), "Improve recovery", Some(1)).unwrap();
        workflow.state = WorkflowState::CycleComplete;
        save(dir.path(), &workflow).unwrap();
        let completed = advance(dir.path(), &workflow.id).unwrap();
        assert_eq!(completed.state, WorkflowState::Completed);
        assert!(completed.is_terminal());
        assert_eq!(completed.cycles.len(), 1);
        assert!(start(dir.path(), "A separate explicitly started session", Some(1)).is_ok());
    }
    #[test]
    fn aggregate_budget_is_not_reset_by_next_cycle() {
        let dir = project();
        let mut workflow = start(dir.path(), "Improve recovery", None).unwrap();
        workflow.dispatches_used = workflow.max_dispatches as u64;
        assert!(next_cycle(&mut workflow).is_err());
        assert_eq!(workflow.cycles.len(), 1);
        workflow.dispatches_used = 0;
        workflow.created_at = setup::timestamp().saturating_sub(workflow.max_seconds + 1);
        workflow.block("Elapsed-time budget exhausted");
        save(dir.path(), &workflow).unwrap();
        assert!(resume(dir.path(), &workflow.id).is_err());
        assert_eq!(
            load(dir.path(), &workflow.id).unwrap().state,
            WorkflowState::Blocked
        );
    }
    #[test]
    fn stop_before_discovery_confirms_terminal_state_and_preserves_history() {
        let dir = project();
        let workflow = start(dir.path(), "Improve recovery", None).unwrap();
        let stopped = stop(dir.path(), &workflow.id, "User ended this session").unwrap();
        assert_eq!(stopped.state, WorkflowState::Stopped);
        assert!(stopped.stop_requested);
        assert_eq!(stopped.cycles.len(), 1);
        assert!(stop_path(dir.path(), &workflow.id).unwrap().is_file());
        assert!(resume(dir.path(), &workflow.id).is_err());
    }
    #[test]
    fn polling_does_not_spend_dispatches_and_completed_intents_are_counted_once() {
        let dir = project();
        let workflow = start(dir.path(), "Improve recovery", None).unwrap();
        let cycle = workflow.current_cycle();
        let discovery = json!({"id":cycle.discovery_id,"project_id":workflow.project_id,"outputs":[{},{}],"pending":{"idempotency_key":"saved-job"}});
        setup::atomic(
            &setup::safe_path(
                dir.path(),
                &format!("{CONFIG}/discovery/{}.json", cycle.discovery_id),
            )
            .unwrap(),
            setup::json(&discovery).unwrap().as_bytes(),
        )
        .unwrap();
        let retrospective = json!({"pending":{"idempotency_key":cycle.retrospective_id},"output":{"disposition":"no_change"}});
        setup::atomic(
            &setup::safe_path(
                dir.path(),
                &format!("{CONFIG}/retrospectives/{}.json", cycle.retrospective_id),
            )
            .unwrap(),
            setup::json(&retrospective).unwrap().as_bytes(),
        )
        .unwrap();
        for _ in 0..3 {
            assert_eq!(load(dir.path(), &workflow.id).unwrap().dispatches_used, 4);
        }
    }
    #[test]
    fn mismatched_core_and_moved_root_block_before_execution() {
        let dir = project();
        let mut workflow = start(dir.path(), "Improve recovery", None).unwrap();
        workflow.core_version = "unavailable-release".into();
        save(dir.path(), &workflow).unwrap();
        let blocked = advance(dir.path(), &workflow.id).unwrap();
        assert_eq!(blocked.state, WorkflowState::Blocked);
        assert_eq!(blocked.dispatches_used, 0);
        workflow.core_version = setup::VERSION.into();
        workflow.repository_root = PathBuf::from("/a-different-project-root");
        save(dir.path(), &workflow).unwrap();
        assert_eq!(
            advance(dir.path(), &workflow.id).unwrap().state,
            WorkflowState::Blocked
        );
    }
    #[test]
    fn pending_last_dispatch_can_resume_after_both_budgets_expire() {
        let dir = project();
        let mut workflow = start(dir.path(), "Improve recovery", None).unwrap();
        workflow.max_dispatches = 3;
        workflow.created_at = setup::timestamp().saturating_sub(workflow.max_seconds + 1);
        let cycle = workflow.current_cycle();
        let discovery_path = setup::safe_path(
            dir.path(),
            &format!("{CONFIG}/discovery/{}.json", cycle.discovery_id),
        )
        .unwrap();
        let mut record = json!({"id":cycle.discovery_id,"project_id":workflow.project_id,"next_stage":2,"outputs":[{},{}],"pending":{"idempotency_key":"last-allowed-pm"}});
        setup::atomic(&discovery_path, setup::json(&record).unwrap().as_bytes()).unwrap();
        workflow.block("PM response unknown; reconcile saved key");
        save(dir.path(), &workflow).unwrap();
        let mut resumed = resume(dir.path(), &workflow.id).unwrap();
        assert_eq!(resumed.state, WorkflowState::Discovery);
        assert_eq!(resumed.dispatches_used, 3);
        assert_eq!(
            next_work_kind(dir.path(), &resumed, WorkflowState::Discovery).unwrap(),
            BudgetUse::Reconciliation
        );
        assert!(check_budget(&resumed, BudgetUse::Reconciliation).is_ok());
        assert!(check_budget(&resumed, BudgetUse::Agent).is_err());
        assert!(next_cycle(&mut resumed).is_err());
        record["pending"] = Value::Null;
        setup::atomic(&discovery_path, setup::json(&record).unwrap().as_bytes()).unwrap();
        resumed.block("Would require a new dispatch");
        save(dir.path(), &resumed).unwrap();
        assert!(resume(dir.path(), &resumed.id).is_err());
    }
    #[test]
    fn new_checks_use_time_budget_while_lookup_does_not() {
        let dir = project();
        let mut workflow = start(dir.path(), "Improve recovery", None).unwrap();
        workflow.dispatches_used = workflow.max_dispatches as u64;
        assert!(check_budget(&workflow, BudgetUse::Check).is_ok());
        assert!(check_budget(&workflow, BudgetUse::Agent).is_err());
        workflow.created_at = setup::timestamp().saturating_sub(workflow.max_seconds + 1);
        assert!(check_budget(&workflow, BudgetUse::Check).is_err());
        assert!(check_budget(&workflow, BudgetUse::Reconciliation).is_ok());
    }
    #[test]
    fn discovery_requires_the_explicitly_adopted_project_release_and_profile() {
        let dir = project();
        let lock_path = dir.path().join(format!("{CONFIG}/lock.json"));
        let mut lock = setup::load_lock(dir.path()).unwrap();
        let original = lock.clone();
        lock["core"] = json!("unadopted-release");
        setup::atomic(&lock_path, setup::json(&lock).unwrap().as_bytes()).unwrap();
        assert!(start(dir.path(), "Improve recovery", None).is_err());
        setup::atomic(&lock_path, setup::json(&original).unwrap().as_bytes()).unwrap();
        let workflow = start(dir.path(), "Improve recovery", None).unwrap();
        let mut profile = setup::load_profile(dir.path()).unwrap();
        profile.product_brief.push_str(" changed outside setup");
        setup::atomic(
            &dir.path().join(format!("{CONFIG}/profile.json")),
            setup::json(&profile).unwrap().as_bytes(),
        )
        .unwrap();
        assert_eq!(
            advance(dir.path(), &workflow.id).unwrap().state,
            WorkflowState::Blocked
        );
    }
}
