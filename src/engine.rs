//! Durable, project-scoped workflow rules. Adapters supply execution and verify human decisions.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn nonempty(value: &str, name: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{name} must not be empty");
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    pub id: String,
    pub description: String,
    pub required: bool,
    pub check_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journey {
    pub id: String,
    pub description: String,
    pub criterion_ids: Vec<String>,
    pub check_ids: Vec<String>,
    pub required: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub revision: String,
    pub title: String,
    pub scope: String,
    pub criteria: Vec<Criterion>,
    pub journeys: Vec<Journey>,
    pub required_checks: Vec<String>,
}
impl Proposal {
    pub fn validate(&self) -> Result<()> {
        nonempty(&self.revision, "proposal revision")?;
        nonempty(&self.title, "title")?;
        nonempty(&self.scope, "scope")?;
        ensure!(
            !self.criteria.is_empty(),
            "proposal needs acceptance criteria"
        );
        let mut ids = HashSet::new();
        for c in &self.criteria {
            nonempty(&c.id, "criterion ID")?;
            nonempty(&c.description, "criterion description")?;
            ensure!(ids.insert(c.id.clone()), "duplicate criterion {}", c.id);
            for id in &c.check_ids {
                nonempty(id, "check ID")?;
            }
        }
        ensure!(
            self.criteria.iter().any(|c| c.required),
            "proposal needs a mandatory criterion"
        );
        let mut journey_ids = HashSet::new();
        for j in &self.journeys {
            nonempty(&j.id, "journey ID")?;
            nonempty(&j.description, "journey description")?;
            ensure!(journey_ids.insert(&j.id), "duplicate journey {}", j.id);
            ensure!(
                !j.criterion_ids.is_empty() && j.criterion_ids.iter().all(|id| ids.contains(id)),
                "journey {} must link known criteria",
                j.id
            );
            ensure!(
                !j.required || !j.check_ids.is_empty(),
                "required journey {} needs executable evidence checks",
                j.id
            );
            for id in &j.check_ids {
                nonempty(id, "journey check ID")?;
            }
        }
        for id in &self.required_checks {
            nonempty(id, "required check ID")?;
        }
        Ok(())
    }
    pub fn all_required_checks(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .required_checks
            .iter()
            .cloned()
            .chain(
                self.criteria
                    .iter()
                    .filter(|c| c.required)
                    .flat_map(|c| c.check_ids.clone()),
            )
            .chain(
                self.journeys
                    .iter()
                    .filter(|j| j.required)
                    .flat_map(|j| j.check_ids.clone()),
            )
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    Build,
    Accept,
    Merge,
    Release,
    HarnessAdopt,
    RequestChanges,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionClaim {
    pub id: String,
    pub project_id: String,
    pub run_id: String,
    pub actor: String,
    pub decided_at: String,
    pub artifact_revision: String,
    pub source: String,
    pub action: DecisionAction,
}
/// Implement only at an authenticated human-review adapter boundary. A role result is not a decision.
pub trait DecisionVerifier {
    fn verify(&self, claim: &DecisionClaim) -> Result<()>;
}
/// Deliberately not deserializable: incoming JSON cannot mint human authority.
#[derive(Debug)]
pub struct VerifiedDecision(DecisionClaim);
impl VerifiedDecision {
    pub fn verify(claim: DecisionClaim, verifier: &impl DecisionVerifier) -> Result<Self> {
        for (name, value) in [
            ("decision ID", &claim.id),
            ("project", &claim.project_id),
            ("run", &claim.run_id),
            ("human actor", &claim.actor),
            ("decision time", &claim.decided_at),
            ("artifact revision", &claim.artifact_revision),
            ("attributable source", &claim.source),
        ] {
            nonempty(value, name)?;
        }
        verifier.verify(&claim)?;
        Ok(Self(claim))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    pub max_iterations: u32,
    pub max_dispatches: u32,
    pub max_elapsed_seconds: u64,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            max_dispatches: 30,
            max_elapsed_seconds: 14_400,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    AwaitingBuildApproval,
    Planning,
    Implementing,
    Checking,
    Reviewing,
    AwaitingAcceptance,
    Accepted,
    Blocked,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Planner,
    Implementer,
    Reviewer,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dispatch {
    pub id: String,
    pub context_id: String,
    pub role: Role,
    pub iteration: u32,
    pub input: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub id: String,
    pub criterion_ids: Vec<String>,
    pub files: Vec<String>,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerResult {
    pub summary: String,
    pub tasks: Vec<PlanTask>,
    pub required_checks: Vec<String>,
    pub guidance_impact: String,
    pub risks: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementerResult {
    pub summary: String,
    pub candidate_revision: String,
    pub diff_ref: String,
    pub guidance_diff_ref: String,
    pub known_gaps: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionVerdict {
    pub criterion_id: String,
    pub passed: bool,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub criterion_id: String,
    pub expected: String,
    pub observed: String,
    pub severity: String,
    pub cause: String,
    pub evidence_ref: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerResult {
    pub summary: String,
    pub candidate_revision: String,
    pub guidance_consistent: bool,
    pub verdicts: Vec<CriterionVerdict>,
    pub findings: Vec<Finding>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", content = "result", rename_all = "snake_case")]
pub enum RoleResult {
    Planner(PlannerResult),
    Implementer(ImplementerResult),
    Reviewer(ReviewerResult),
}
impl RoleResult {
    pub fn role(&self) -> Role {
        match self {
            Self::Planner(_) => Role::Planner,
            Self::Implementer(_) => Role::Implementer,
            Self::Reviewer(_) => Role::Reviewer,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Pending,
    Completed,
    Unavailable,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Passed,
    Failed,
    Blocked,
    NotRun,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub id: String,
    pub revision: String,
    pub execution: ExecutionState,
    pub outcome: CheckOutcome,
    pub evidence_refs: Vec<String>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Iteration {
    pub number: u32,
    pub dispatches: Vec<Dispatch>,
    pub plan: Option<PlannerResult>,
    pub implementation: Option<ImplementerResult>,
    pub checks: Vec<CheckResult>,
    pub review: Option<ReviewerResult>,
    pub unresolved: Vec<String>,
}
impl Iteration {
    fn new(number: u32) -> Self {
        Self {
            number,
            dispatches: vec![],
            plan: None,
            implementation: None,
            checks: vec![],
            review: None,
            unresolved: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub at: u64,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub schema_version: u32,
    pub id: String,
    pub project_id: String,
    pub proposal: Proposal,
    pub profile_snapshot: Value,
    pub pins: Value,
    pub stage: Stage,
    pub iteration: u32,
    pub iterations: Vec<Iteration>,
    pub budget: Budget,
    pub dispatches_used: u32,
    pub active_dispatch: Option<Dispatch>,
    pub decisions: Vec<DecisionClaim>,
    pub events: Vec<Event>,
    pub created_at: u64,
    pub blocked_reason: Option<String>,
    pub resume_stage: Option<Stage>,
    #[serde(skip)]
    authority_verified: bool,
}
impl Run {
    pub fn new(
        project_id: String,
        proposal: Proposal,
        profile_snapshot: Value,
        pins: Value,
        budget: Budget,
    ) -> Result<Self> {
        nonempty(&project_id, "project ID")?;
        proposal.validate()?;
        ensure!(
            budget.max_iterations > 0
                && budget.max_dispatches > 0
                && budget.max_elapsed_seconds > 0,
            "budgets must be positive"
        );
        Ok(Self {
            schema_version: 1,
            id: Uuid::new_v4().to_string(),
            project_id,
            proposal,
            profile_snapshot,
            pins,
            stage: Stage::AwaitingBuildApproval,
            iteration: 1,
            iterations: vec![Iteration::new(1)],
            budget,
            dispatches_used: 0,
            active_dispatch: None,
            decisions: vec![],
            events: vec![Event {
                at: now(),
                message: "Proposal recorded; awaiting attributable build approval".into(),
            }],
            created_at: now(),
            blocked_reason: None,
            resume_stage: None,
            authority_verified: true,
        })
    }
    pub fn current(&self) -> &Iteration {
        self.iterations.last().expect("run has an iteration")
    }
    fn current_mut(&mut self) -> &mut Iteration {
        self.iterations.last_mut().expect("run has an iteration")
    }
    fn event(&mut self, message: impl Into<String>) {
        self.events.push(Event {
            at: now(),
            message: message.into(),
        });
    }
    pub fn candidate_revision(&self) -> Option<&str> {
        self.current()
            .implementation
            .as_ref()
            .map(|i| i.candidate_revision.as_str())
    }
    pub fn has_authority(&self, action: DecisionAction, revision: &str) -> bool {
        self.authority_verified
            && self.decisions.iter().any(|d| {
                d.action == action
                    && d.artifact_revision == revision
                    && d.project_id == self.project_id
                    && d.run_id == self.id
            })
    }
    /// Re-establish trust after loading persisted claims before executing authorised work.
    pub fn reverify_decisions(&mut self, verifier: &impl DecisionVerifier) -> Result<()> {
        self.authority_verified = false;
        for claim in &self.decisions {
            VerifiedDecision::verify(claim.clone(), verifier)?;
        }
        self.authority_verified = true;
        Ok(())
    }
    pub fn apply_decision(&mut self, decision: VerifiedDecision) -> Result<()> {
        let d = decision.0;
        ensure!(
            self.authority_verified || self.decisions.is_empty(),
            "saved decisions must be reverified before applying further authority"
        );
        ensure!(
            d.project_id == self.project_id && d.run_id == self.id,
            "decision belongs to another project or run"
        );
        if let Some(existing) = self.decisions.iter().find(|v| v.id == d.id) {
            ensure!(
                serde_json::to_value(existing)? == serde_json::to_value(&d)?,
                "decision identity collision"
            );
            return Ok(());
        }
        ensure!(
            self.stage != Stage::Cancelled,
            "cancelled runs cannot receive authority"
        );
        match d.action {
            DecisionAction::Build => {
                ensure!(
                    self.stage == Stage::AwaitingBuildApproval,
                    "build decision is not applicable at this stage"
                );
                ensure!(
                    d.artifact_revision == self.proposal.revision,
                    "approval does not match proposal revision"
                );
                self.stage = Stage::Planning;
            }
            DecisionAction::Accept => {
                ensure!(
                    self.stage == Stage::AwaitingAcceptance,
                    "feature is not ready for human acceptance"
                );
                ensure!(
                    self.candidate_revision() == Some(d.artifact_revision.as_str()),
                    "acceptance is for a stale revision"
                );
                ensure!(
                    self.readiness_failures(
                        self.current()
                            .review
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("missing review"))?
                    )
                    .is_empty(),
                    "evidence no longer satisfies acceptance"
                );
                self.stage = Stage::Accepted;
            }
            DecisionAction::Merge | DecisionAction::Release => {
                ensure!(
                    self.stage == Stage::Accepted,
                    "feature must be accepted before merge or release authority"
                );
                ensure!(
                    self.candidate_revision() == Some(d.artifact_revision.as_str()),
                    "authority is for a stale revision"
                );
            }
            DecisionAction::HarnessAdopt => {
                bail!(
                    "harness adoption requires a separate evaluated harness candidate; feature authority cannot adopt harness changes"
                );
            }
            DecisionAction::RequestChanges => {
                ensure!(
                    self.stage == Stage::AwaitingAcceptance,
                    "changes may be requested on the reviewed candidate"
                );
                ensure!(
                    self.candidate_revision() == Some(d.artifact_revision.as_str()),
                    "change request is for a stale revision"
                );
                self.next_iteration(vec![format!("Human change request: {}", d.source)]);
            }
        }
        self.event(format!(
            "Verified {:?} decision {} from {}",
            d.action, d.id, d.source
        ));
        self.decisions.push(d);
        self.authority_verified = true;
        Ok(())
    }
    fn budget_problem(&self) -> Option<String> {
        if self.iteration > self.budget.max_iterations {
            Some("Iteration budget exhausted".into())
        } else if self.dispatches_used >= self.budget.max_dispatches {
            Some("Dispatch budget exhausted".into())
        } else if now().saturating_sub(self.created_at) >= self.budget.max_elapsed_seconds {
            Some("Elapsed-time budget exhausted".into())
        } else {
            None
        }
    }
    pub fn begin_role(&mut self, role: Role) -> Result<Dispatch> {
        ensure!(
            self.active_dispatch.is_none(),
            "a dispatch is already active; recover it before retrying"
        );
        ensure!(
            self.has_authority(DecisionAction::Build, &self.proposal.revision),
            "verified build approval is missing"
        );
        let expected = match role {
            Role::Planner => Stage::Planning,
            Role::Implementer => Stage::Implementing,
            Role::Reviewer => Stage::Reviewing,
        };
        ensure!(
            self.stage == expected,
            "cannot dispatch {:?} from {:?}",
            role,
            self.stage
        );
        if let Some(reason) = self.budget_problem() {
            self.block(reason.clone());
            bail!(reason);
        }
        let dispatch = Dispatch {
            id: Uuid::new_v4().to_string(),
            context_id: Uuid::new_v4().to_string(),
            role,
            iteration: self.iteration,
            input: json!({ "project_id": self.project_id, "run_id": self.id, "proposal": self.proposal, "profile": self.profile_snapshot, "pins": self.pins, "iteration": self.current(), "previous_iteration": if self.iterations.len() > 1 { self.iterations.get(self.iterations.len()-2) } else { None }, "remaining_dispatches": self.budget.max_dispatches-self.dispatches_used, "reasoning": if role == Role::Planner { "high" } else { "configured_default" }, "writer": role == Role::Implementer }),
        };
        self.current_mut().dispatches.push(dispatch.clone());
        self.active_dispatch = Some(dispatch.clone());
        self.dispatches_used += 1;
        self.event(format!(
            "Dispatched {:?} in fresh context {}",
            role, dispatch.context_id
        ));
        Ok(dispatch)
    }
    pub fn complete_role(&mut self, dispatch_id: &str, result: RoleResult) -> Result<()> {
        ensure!(
            self.has_authority(DecisionAction::Build, &self.proposal.revision),
            "verified build approval is missing"
        );
        let active = self
            .active_dispatch
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no active dispatch"))?;
        ensure!(
            active.id == dispatch_id
                && active.role == result.role()
                && active.iteration == self.iteration,
            "result does not match active handoff"
        );
        let expected = match result.role() {
            Role::Planner => Stage::Planning,
            Role::Implementer => Stage::Implementing,
            Role::Reviewer => Stage::Reviewing,
        };
        ensure!(
            self.stage == expected,
            "run is paused or no longer at the dispatched stage"
        );
        match result {
            RoleResult::Planner(plan) => {
                nonempty(&plan.summary, "plan summary")?;
                nonempty(&plan.guidance_impact, "guidance impact")?;
                ensure!(!plan.tasks.is_empty(), "plan has no tasks");
                let known: HashSet<_> = self
                    .proposal
                    .criteria
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect();
                let mut task_ids = HashSet::new();
                for t in &plan.tasks {
                    nonempty(&t.id, "task ID")?;
                    nonempty(&t.description, "task description")?;
                    ensure!(task_ids.insert(&t.id), "duplicate task ID");
                    ensure!(
                        !t.criterion_ids.is_empty()
                            && t.criterion_ids.iter().all(|id| known.contains(id.as_str())),
                        "tasks must map to approved criteria"
                    );
                }
                for c in self.proposal.criteria.iter().filter(|c| c.required) {
                    ensure!(
                        plan.tasks.iter().any(|t| t.criterion_ids.contains(&c.id)),
                        "plan omits mandatory criterion {}",
                        c.id
                    );
                }
                for id in self.proposal.all_required_checks() {
                    ensure!(
                        plan.required_checks.contains(&id),
                        "plan removes required check {id}"
                    );
                }
                self.current_mut().plan = Some(plan);
                self.stage = Stage::Implementing;
            }
            RoleResult::Implementer(implementation) => {
                nonempty(&implementation.summary, "implementation summary")?;
                nonempty(&implementation.candidate_revision, "candidate revision")?;
                nonempty(&implementation.diff_ref, "candidate diff")?;
                nonempty(
                    &implementation.guidance_diff_ref,
                    "guidance diff or explicit no-change artifact",
                )?;
                self.current_mut().implementation = Some(implementation);
                self.stage = Stage::Checking;
            }
            RoleResult::Reviewer(review) => {
                nonempty(&review.summary, "review summary")?;
                ensure!(
                    Some(review.candidate_revision.as_str()) == self.candidate_revision(),
                    "review does not match exact candidate revision"
                );
                let mut ids = HashSet::new();
                for v in &review.verdicts {
                    ensure!(
                        self.proposal
                            .criteria
                            .iter()
                            .any(|c| c.id == v.criterion_id),
                        "review contains unknown criterion"
                    );
                    ensure!(ids.insert(&v.criterion_id), "duplicate criterion verdict");
                }
                let failures = self.readiness_failures(&review);
                self.current_mut().review = Some(review);
                self.current_mut().unresolved = failures.clone();
                self.active_dispatch = None;
                if failures.is_empty() {
                    self.stage = Stage::AwaitingAcceptance;
                } else {
                    self.next_iteration(failures);
                }
            }
        }
        self.active_dispatch = None;
        self.event(format!("Handoff completed; stage {:?}", self.stage));
        Ok(())
    }
    pub fn record_check(&mut self, result: CheckResult) -> Result<()> {
        ensure!(
            self.has_authority(DecisionAction::Build, &self.proposal.revision),
            "verified build approval is missing"
        );
        ensure!(
            self.stage == Stage::Checking,
            "checks can be recorded only for the frozen candidate"
        );
        nonempty(&result.id, "check ID")?;
        ensure!(
            self.candidate_revision() == Some(result.revision.as_str()),
            "check evidence is for a different revision"
        );
        ensure!(
            result.outcome != CheckOutcome::Passed
                || (result.execution == ExecutionState::Completed
                    && !result.evidence_refs.is_empty()),
            "passed outcome requires completed execution and evidence"
        );
        ensure!(
            result.evidence_refs.iter().all(|r| !r.trim().is_empty()),
            "blank evidence reference"
        );
        ensure!(
            result.execution == ExecutionState::Completed
                || matches!(result.outcome, CheckOutcome::Blocked | CheckOutcome::NotRun),
            "incomplete execution cannot claim a test outcome"
        );
        if let Some(index) = self.current().checks.iter().position(|v| v.id == result.id) {
            self.current_mut().checks[index] = result;
        } else {
            self.current_mut().checks.push(result);
        }
        Ok(())
    }
    pub fn required_checks(&self) -> Vec<String> {
        let mut ids = self.proposal.all_required_checks();
        if let Some(plan) = &self.current().plan {
            ids.extend(plan.required_checks.iter().cloned());
        }
        ids.sort();
        ids.dedup();
        ids
    }
    pub fn finish_checks(&mut self) -> Result<()> {
        ensure!(self.stage == Stage::Checking, "not at the checking stage");
        let missing: Vec<_> = self
            .required_checks()
            .into_iter()
            .filter(|id| {
                !self.current().checks.iter().any(|c| {
                    &c.id == id
                        && c.execution == ExecutionState::Completed
                        && matches!(c.outcome, CheckOutcome::Passed | CheckOutcome::Failed)
                })
            })
            .collect();
        if !missing.is_empty() {
            let reason = format!(
                "Required checks unavailable or not executed: {}",
                missing.join(", ")
            );
            self.block(reason.clone());
            bail!(reason);
        }
        self.stage = Stage::Reviewing;
        self.event("Checks collected for independent review");
        Ok(())
    }
    pub fn readiness_failures(&self, review: &ReviewerResult) -> Vec<String> {
        let mut failures = vec![];
        if self.candidate_revision() != Some(review.candidate_revision.as_str()) {
            failures.push("candidate_revision".into());
        }
        for id in self.required_checks() {
            if !self.current().checks.iter().any(|c| {
                c.id == id
                    && c.revision == review.candidate_revision
                    && c.execution == ExecutionState::Completed
                    && c.outcome == CheckOutcome::Passed
                    && !c.evidence_refs.is_empty()
            }) {
                failures.push(format!("check:{id}"));
            }
        }
        let mut evidence: HashSet<&str> = self
            .current()
            .checks
            .iter()
            .filter(|c| {
                c.revision == review.candidate_revision && c.execution == ExecutionState::Completed
            })
            .flat_map(|c| c.evidence_refs.iter().map(String::as_str))
            .collect();
        if let Some(i) = &self.current().implementation {
            evidence.insert(i.diff_ref.as_str());
            evidence.insert(i.guidance_diff_ref.as_str());
        }
        for c in self.proposal.criteria.iter().filter(|c| c.required) {
            if !review.verdicts.iter().any(|v| {
                v.criterion_id == c.id
                    && v.passed
                    && !v.evidence_refs.is_empty()
                    && v.evidence_refs
                        .iter()
                        .all(|r| evidence.contains(r.as_str()))
            }) {
                failures.push(c.id.clone());
            }
        }
        if !review.guidance_consistent {
            failures.push("repository_guidance".into());
        }
        for f in &review.findings {
            failures.push(format!("finding:{}:{}", f.criterion_id, f.observed));
        }
        failures.sort();
        failures.dedup();
        failures
    }
    fn next_iteration(&mut self, unresolved: Vec<String>) {
        let unchanged = self.iterations.len() >= 2
            && self.iterations[self.iterations.len() - 2].unresolved == unresolved
            && serde_json::to_value(&self.iterations[self.iterations.len() - 2].plan).ok()
                == serde_json::to_value(&self.current().plan).ok();
        self.current_mut().unresolved = unresolved.clone();
        self.iteration += 1;
        let mut iteration = Iteration::new(self.iteration);
        iteration.unresolved = unresolved;
        self.iterations.push(iteration);
        self.stage = Stage::Planning;
        self.event("Unmet criteria returned to technical planning");
        if let Some(reason) = self.budget_problem() {
            self.block(reason);
        } else if unchanged {
            self.block("Repeated unchanged review failures; record a viable next experiment before resuming".into());
        }
    }
    pub fn block(&mut self, reason: String) {
        if matches!(self.stage, Stage::Accepted | Stage::Cancelled) {
            return;
        }
        if self.stage != Stage::Blocked {
            self.resume_stage = Some(self.stage);
        }
        self.stage = Stage::Blocked;
        self.blocked_reason = Some(reason.clone());
        self.event(format!("Blocked: {reason}"));
    }
    pub fn resume(&mut self) -> Result<()> {
        ensure!(self.stage == Stage::Blocked, "run is not blocked");
        ensure!(
            self.active_dispatch.is_some() || self.budget_problem().is_none(),
            "budget remains exhausted; record an explicitly authorised budget change first"
        );
        // Preserve any active dispatch so the adapter can look it up using its original identity.
        self.stage = self
            .resume_stage
            .take()
            .ok_or_else(|| anyhow::anyhow!("missing resume point"))?;
        self.blocked_reason = None;
        self.event("Resumed from recorded stage");
        Ok(())
    }
    /// Operator recovery is explicit because the external command may already have performed work.
    pub fn abandon_dispatch(&mut self, reason: String) -> Result<()> {
        nonempty(&reason, "recovery reason")?;
        let dispatch = self
            .active_dispatch
            .take()
            .ok_or_else(|| anyhow::anyhow!("no dispatch to recover"))?;
        self.event(format!(
            "Dispatch {} abandoned after reconciliation: {reason}",
            dispatch.id
        ));
        Ok(())
    }
    pub fn cancel(&mut self, reason: String) {
        self.stage = Stage::Cancelled;
        self.event(format!(
            "Cancelled: {reason}; runtime adapter must terminate any active process"
        ));
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("state path has no parent"))?;
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".run-{}.tmp", Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
    pub fn load(path: &Path) -> Result<Self> {
        let run: Self = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(run.schema_version == 1, "unsupported run schema");
        run.proposal.validate()?;
        ensure!(
            !run.iterations.is_empty()
                && run
                    .iterations
                    .last()
                    .is_some_and(|i| i.number == run.iteration),
            "invalid iteration history"
        );
        ensure!(
            run.decisions
                .iter()
                .all(|d| d.project_id == run.project_id && d.run_id == run.id),
            "cross-project decision in run storage"
        );
        Ok(run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Trusted;
    impl DecisionVerifier for Trusted {
        fn verify(&self, d: &DecisionClaim) -> Result<()> {
            ensure!(d.source == "human-session:verified", "untrusted source");
            Ok(())
        }
    }
    fn run() -> Run {
        Run::new(
            "fixture".into(),
            Proposal {
                revision: "proposal-1".into(),
                title: "Recovery".into(),
                scope: "Retry within current journey".into(),
                criteria: vec![Criterion {
                    id: "C1".into(),
                    description: "Retry preserves data".into(),
                    required: true,
                    check_ids: vec!["unit".into()],
                }],
                journeys: vec![Journey {
                    id: "J1".into(),
                    description: "Retry".into(),
                    criterion_ids: vec!["C1".into()],
                    check_ids: vec!["journey".into()],
                    required: true,
                }],
                required_checks: vec!["build".into()],
            },
            json!({"project":"fixture"}),
            json!({"core":"0.1.0","templates":{"planner":"Plan with high reasoning"}}),
            Budget::default(),
        )
        .unwrap()
    }
    fn decision(run: &Run, action: DecisionAction, revision: &str) -> DecisionClaim {
        DecisionClaim {
            id: Uuid::new_v4().to_string(),
            project_id: run.project_id.clone(),
            run_id: run.id.clone(),
            actor: "human".into(),
            decided_at: "2026-09-08T10:00:00Z".into(),
            artifact_revision: revision.into(),
            source: "human-session:verified".into(),
            action,
        }
    }
    fn approve(run: &mut Run) {
        let d = decision(run, DecisionAction::Build, &run.proposal.revision);
        run.apply_decision(VerifiedDecision::verify(d, &Trusted).unwrap())
            .unwrap();
    }
    fn plan() -> RoleResult {
        RoleResult::Planner(PlannerResult {
            summary: "Implement retry".into(),
            tasks: vec![PlanTask {
                id: "T1".into(),
                criterion_ids: vec!["C1".into()],
                files: vec!["retry.rs".into()],
                description: "Preserve data on retry".into(),
            }],
            required_checks: vec!["build".into(), "unit".into(), "journey".into()],
            guidance_impact: "No durable guidance changes".into(),
            risks: vec![],
        })
    }
    fn implement() -> RoleResult {
        RoleResult::Implementer(ImplementerResult {
            summary: "Retry fixed".into(),
            candidate_revision: "code-1".into(),
            diff_ref: "artifact:diff".into(),
            guidance_diff_ref: "artifact:guidance".into(),
            known_gaps: vec![],
        })
    }
    fn checking(run: &mut Run) {
        let d = run.begin_role(Role::Planner).unwrap();
        run.complete_role(&d.id, plan()).unwrap();
        let d = run.begin_role(Role::Implementer).unwrap();
        run.complete_role(&d.id, implement()).unwrap();
    }
    fn check(id: &str, outcome: CheckOutcome) -> CheckResult {
        CheckResult {
            id: id.into(),
            revision: "code-1".into(),
            execution: ExecutionState::Completed,
            outcome,
            evidence_refs: vec![format!("artifact:{id}")],
            error: None,
        }
    }
    fn reviewing(run: &mut Run, outcome: CheckOutcome) {
        checking(run);
        for id in ["build", "unit", "journey"] {
            run.record_check(check(id, outcome)).unwrap();
        }
        run.finish_checks().unwrap();
    }
    fn review(passed: bool) -> RoleResult {
        RoleResult::Reviewer(ReviewerResult {
            summary: "Reviewed candidate".into(),
            candidate_revision: "code-1".into(),
            guidance_consistent: true,
            verdicts: vec![CriterionVerdict {
                criterion_id: "C1".into(),
                passed,
                evidence_refs: vec!["artifact:journey".into()],
            }],
            findings: vec![],
        })
    }
    #[test]
    fn rejects_missing_and_wrong_project_approval() {
        let mut r = run();
        assert!(r.begin_role(Role::Planner).is_err());
        let mut d = decision(&r, DecisionAction::Build, "proposal-1");
        d.project_id = "another-project".into();
        assert!(
            r.apply_decision(VerifiedDecision::verify(d, &Trusted).unwrap())
                .is_err()
        );
        let d = decision(&r, DecisionAction::Build, "superseded");
        assert!(
            r.apply_decision(VerifiedDecision::verify(d, &Trusted).unwrap())
                .is_err()
        );
        assert_eq!(r.stage, Stage::AwaitingBuildApproval);
    }
    #[test]
    fn untrusted_claim_cannot_mint_authority() {
        let r = run();
        let mut d = decision(&r, DecisionAction::Build, "proposal-1");
        d.source = "agent-authored-approved_by".into();
        assert!(VerifiedDecision::verify(d, &Trusted).is_err());
    }
    #[test]
    fn three_fresh_contexts_and_acceptance_does_not_grant_merge() {
        let mut r = run();
        approve(&mut r);
        reviewing(&mut r, CheckOutcome::Passed);
        let d = r.begin_role(Role::Reviewer).unwrap();
        r.complete_role(&d.id, review(true)).unwrap();
        assert_eq!(r.stage, Stage::AwaitingAcceptance);
        let contexts: HashSet<_> = r
            .current()
            .dispatches
            .iter()
            .map(|d| &d.context_id)
            .collect();
        assert_eq!(contexts.len(), 3);
        let d = decision(&r, DecisionAction::Accept, "code-1");
        r.apply_decision(VerifiedDecision::verify(d, &Trusted).unwrap())
            .unwrap();
        assert_eq!(r.stage, Stage::Accepted);
        assert!(!r.has_authority(DecisionAction::Merge, "code-1"));
        assert!(!r.has_authority(DecisionAction::Release, "code-1"));
        let d = decision(&r, DecisionAction::Merge, "old-code");
        assert!(
            r.apply_decision(VerifiedDecision::verify(d, &Trusted).unwrap())
                .is_err()
        );
        let d = decision(&r, DecisionAction::Merge, "code-1");
        r.apply_decision(VerifiedDecision::verify(d, &Trusted).unwrap())
            .unwrap();
        assert!(r.has_authority(DecisionAction::Merge, "code-1"));
    }
    #[test]
    fn rejects_wrong_role_stale_evidence_and_stale_review() {
        let mut r = run();
        approve(&mut r);
        let d = r.begin_role(Role::Planner).unwrap();
        assert!(r.complete_role(&d.id, implement()).is_err());
        assert!(r.begin_role(Role::Planner).is_err());
        r.complete_role(&d.id, plan()).unwrap();
        let d = r.begin_role(Role::Implementer).unwrap();
        r.complete_role(&d.id, implement()).unwrap();
        let mut stale = check("build", CheckOutcome::Passed);
        stale.revision = "old-code".into();
        assert!(r.record_check(stale).is_err());
        for id in ["build", "unit", "journey"] {
            r.record_check(check(id, CheckOutcome::Passed)).unwrap();
        }
        r.finish_checks().unwrap();
        let d = r.begin_role(Role::Reviewer).unwrap();
        let RoleResult::Reviewer(mut result) = review(true) else {
            unreachable!()
        };
        result.candidate_revision = "old-code".into();
        assert!(
            r.complete_role(&d.id, RoleResult::Reviewer(result))
                .is_err()
        );
        assert_eq!(r.stage, Stage::Reviewing);
    }
    #[test]
    fn missing_required_execution_blocks_and_resumes_at_checks() {
        let mut r = run();
        approve(&mut r);
        checking(&mut r);
        let mut unavailable = check("journey", CheckOutcome::NotRun);
        unavailable.execution = ExecutionState::Unavailable;
        r.record_check(unavailable).unwrap();
        assert!(r.finish_checks().is_err());
        assert_eq!(r.stage, Stage::Blocked);
        assert_eq!(r.resume_stage, Some(Stage::Checking));
        r.resume().unwrap();
        for id in ["build", "unit", "journey"] {
            r.record_check(check(id, CheckOutcome::Passed)).unwrap();
        }
        r.finish_checks().unwrap();
        assert_eq!(r.stage, Stage::Reviewing);
    }
    #[test]
    fn failed_check_cannot_be_averaged_away_by_reviewer() {
        let mut r = run();
        approve(&mut r);
        reviewing(&mut r, CheckOutcome::Failed);
        let d = r.begin_role(Role::Reviewer).unwrap();
        r.complete_role(&d.id, review(true)).unwrap();
        assert_eq!(r.stage, Stage::Planning);
        assert_eq!(r.iteration, 2);
        assert!(r.iterations[0].unresolved.contains(&"check:journey".into()));
    }
    #[test]
    fn missing_criterion_or_invented_evidence_returns_to_planner() {
        for missing in [true, false] {
            let mut r = run();
            approve(&mut r);
            reviewing(&mut r, CheckOutcome::Passed);
            let d = r.begin_role(Role::Reviewer).unwrap();
            let RoleResult::Reviewer(mut result) = review(true) else {
                unreachable!()
            };
            if missing {
                result.verdicts.clear();
            } else {
                result.verdicts[0].evidence_refs = vec!["invented-proof".into()];
            }
            r.complete_role(&d.id, RoleResult::Reviewer(result))
                .unwrap();
            assert_eq!(r.stage, Stage::Planning);
            assert!(r.current().unresolved.contains(&"C1".into()));
        }
    }
    #[test]
    fn plans_cannot_drop_required_criteria_or_checks() {
        let mut r = run();
        approve(&mut r);
        let d = r.begin_role(Role::Planner).unwrap();
        let RoleResult::Planner(mut result) = plan() else {
            unreachable!()
        };
        result.required_checks.clear();
        assert!(r.complete_role(&d.id, RoleResult::Planner(result)).is_err());
        assert_eq!(r.stage, Stage::Planning);
    }
    #[test]
    fn interrupted_dispatch_is_persisted_and_requires_reconciliation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut r = run();
        approve(&mut r);
        let d = r.begin_role(Role::Planner).unwrap();
        r.save(&path).unwrap();
        let mut restored = Run::load(&path).unwrap();
        assert_eq!(restored.active_dispatch.as_ref().unwrap().id, d.id);
        assert_eq!(restored.profile_snapshot, r.profile_snapshot);
        assert_eq!(restored.pins, r.pins);
        assert!(restored.begin_role(Role::Planner).is_err());
        restored.block("Command interrupted".into());
        restored.resume().unwrap();
        assert!(restored.begin_role(Role::Planner).is_err());
        restored.block("Reconcile command before retry".into());
        restored
            .abandon_dispatch("Operator checked that the previous process stopped".into())
            .unwrap();
        restored.resume().unwrap();
        restored.reverify_decisions(&Trusted).unwrap();
        assert_ne!(
            restored.begin_role(Role::Planner).unwrap().context_id,
            d.context_id
        );
    }
    #[test]
    fn budgets_and_cancellation_never_imply_success() {
        let mut r = run();
        r.budget.max_dispatches = 1;
        approve(&mut r);
        let d = r.begin_role(Role::Planner).unwrap();
        r.complete_role(&d.id, plan()).unwrap();
        assert!(r.begin_role(Role::Implementer).is_err());
        assert_eq!(r.stage, Stage::Blocked);
        assert!(r.resume().is_err());
        r.cancel("User cancelled".into());
        assert_eq!(r.stage, Stage::Cancelled);
        assert!(r.resume().is_err());
        assert!(r.begin_role(Role::Implementer).is_err());
    }
    #[test]
    fn incomplete_execution_cannot_claim_passed() {
        let mut r = run();
        approve(&mut r);
        checking(&mut r);
        let mut result = check("build", CheckOutcome::Passed);
        result.execution = ExecutionState::Unavailable;
        assert!(r.record_check(result).is_err());
        let mut result = check("build", CheckOutcome::Passed);
        result.evidence_refs.clear();
        assert!(r.record_check(result).is_err());
    }
    #[test]
    fn repeated_unchanged_failures_preserve_blocked_resume_point() {
        let mut r = run();
        approve(&mut r);
        for _ in 0..2 {
            reviewing(&mut r, CheckOutcome::Passed);
            let d = r.begin_role(Role::Reviewer).unwrap();
            r.complete_role(&d.id, review(false)).unwrap();
        }
        assert_eq!(r.stage, Stage::Blocked);
        assert_eq!(r.resume_stage, Some(Stage::Planning));
        assert_eq!(r.iterations.len(), 3);
    }
    #[test]
    fn persisted_claims_require_live_reverification() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut r = run();
        approve(&mut r);
        r.save(&path).unwrap();
        let mut restored = Run::load(&path).unwrap();
        assert!(!restored.has_authority(DecisionAction::Build, "proposal-1"));
        assert!(restored.begin_role(Role::Planner).is_err());
        restored.reverify_decisions(&Trusted).unwrap();
        assert!(restored.begin_role(Role::Planner).is_ok());
    }

    #[test]
    fn revised_plans_can_continue_beyond_two_attempts() {
        let mut r = run();
        approve(&mut r);
        for attempt in 0..3 {
            let d = r.begin_role(Role::Planner).unwrap();
            let RoleResult::Planner(mut p) = plan() else {
                unreachable!()
            };
            p.summary = format!("Experiment {attempt}: a distinct repair strategy");
            r.complete_role(&d.id, RoleResult::Planner(p)).unwrap();
            let d = r.begin_role(Role::Implementer).unwrap();
            r.complete_role(&d.id, implement()).unwrap();
            for id in ["build", "unit", "journey"] {
                r.record_check(check(id, CheckOutcome::Passed)).unwrap();
            }
            r.finish_checks().unwrap();
            let d = r.begin_role(Role::Reviewer).unwrap();
            r.complete_role(&d.id, review(false)).unwrap();
            assert_eq!(r.stage, Stage::Planning);
        }
        assert_eq!(r.iteration, 4);
    }
}
