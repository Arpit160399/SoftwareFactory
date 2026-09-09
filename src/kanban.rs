//! Project-local task projection and durable Notion synchronization.
use crate::{
    adapters,
    engine::Stage,
    setup::{self, CONFIG, CommandSpec},
    workflow,
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub database_id: String,
    pub data_source_id: String,
    #[serde(default = "default_properties")]
    pub properties: BTreeMap<String, String>,
}
pub fn default_properties() -> BTreeMap<String, String> {
    [
        ("name", "Name"),
        ("status", "Status"),
        ("record_key", "Record key"),
        ("project_id", "Project ID"),
        ("workflow_id", "Workflow ID"),
        ("cycle", "Cycle"),
        ("kind", "Type"),
        ("stage", "Stage"),
        ("agent", "Current agent"),
        ("iteration", "Iteration"),
        ("summary", "Summary"),
        ("blocker", "Blocker"),
        ("next_action", "Next action"),
        ("revision", "Artifact revision"),
        ("event_time", "Event time"),
        ("synced_at", "Last synced"),
        ("sequence", "Event sequence"),
        ("review_url", "Review packet"),
    ]
    .into_iter()
    .map(|(a, b)| (a.into(), b.into()))
    .collect()
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        for id in [&self.database_id, &self.data_source_id] {
            ensure!(
                uuid::Uuid::parse_str(id).is_ok(),
                "Notion database and data source IDs must be UUIDs"
            );
        }
        ensure!(
            default_properties().keys().all(|key| self
                .properties
                .get(key)
                .is_some_and(|v| !v.trim().is_empty())),
            "Notion property mapping is incomplete"
        );
        let values: std::collections::HashSet<_> = self.properties.values().collect();
        ensure!(
            values.len() == self.properties.len(),
            "Each Notion field needs a distinct property"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Card {
    pub record_key: String,
    pub project_id: String,
    pub workflow_id: String,
    pub cycle: u32,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub stage: String,
    pub agent: String,
    pub iteration: u32,
    pub summary: String,
    pub blocker: String,
    pub next_action: String,
    pub revision: String,
    pub event_time: u64,
    pub review_url: String,
}
pub fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::AwaitingBuildApproval => "Needs approval",
        Stage::Planning => "Planning",
        Stage::Implementing => "Implementing",
        Stage::Checking => "Checking",
        Stage::Reviewing => "Reviewing",
        Stage::AwaitingAcceptance => "Ready for review",
        Stage::Accepted => "Accepted",
        Stage::Blocked => "Blocked",
        Stage::Cancelled => "Cancelled",
    }
}
fn review_url(root: &Path, run: &str) -> String {
    let path = setup::safe_path(root, &format!("{CONFIG}/runs/{run}/review-sync.json"));
    path.ok()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["reference"].as_str().map(str::to_owned))
        .filter(|s| s.starts_with("https://www.notion.so/") || s.starts_with("https://notion.so/"))
        .unwrap_or_default()
}
pub fn cards(root: &Path) -> Result<Vec<Card>> {
    let mut cards = Vec::new();
    for w in workflow::list(root)? {
        for c in &w.cycles {
            let mut card = Card {
                record_key: format!("{}:{}:discovery:{}", w.project_id, w.id, c.discovery_id),
                project_id: w.project_id.clone(),
                workflow_id: w.id.clone(),
                cycle: c.number,
                kind: "Discovery".into(),
                name: c.question.clone(),
                status: "Discovery".into(),
                stage: "Waiting to research".into(),
                agent: "none".into(),
                iteration: 0,
                summary: String::new(),
                blocker: String::new(),
                next_action: String::new(),
                revision: String::new(),
                event_time: w.created_at,
                review_url: String::new(),
            };
            let path =
                setup::safe_path(root, &format!("{CONFIG}/discovery/{}.json", c.discovery_id))?;
            if path.exists() {
                let d = adapters::discovery_load(root, &c.discovery_id)?;
                let stage = d["next_stage"].as_u64().unwrap_or(0);
                card.stage = [
                    "Research",
                    "Opportunities",
                    "Product planning",
                    "Discovery concluded",
                ][stage.min(3) as usize]
                    .into();
                card.agent = ["research", "opportunities", "product-manager", "none"]
                    [stage.min(3) as usize]
                    .into();
                if d["pending"].is_null() {
                    card.agent = "none".into();
                }
                card.summary = d["outputs"]
                    .as_array()
                    .and_then(|a| a.last())
                    .map(|v| {
                        v["output"]["reason"]
                            .as_str()
                            .unwrap_or("Findings recorded in project history")
                            .to_string()
                    })
                    .unwrap_or_default();
                if stage >= 3 {
                    card.status =
                        if matches!(c.pm_disposition.as_deref(), Some("no_action" | "defer")) {
                            "Deferred"
                        } else {
                            "Done"
                        }
                        .into();
                }
            }
            if c.number == w.current_cycle().number && c.run_id.is_none() {
                match w.state {
                    workflow::WorkflowState::Blocked => {
                        card.status = "Blocked".into();
                        card.blocker = w.blocked_reason.clone().unwrap_or_default();
                        card.next_action = "Resolve the cause, then resume".into();
                    }
                    workflow::WorkflowState::Stopped => card.status = "Cancelled".into(),
                    workflow::WorkflowState::WaitingForOpportunity => {
                        card.next_action = "Resume when there is new product direction".into()
                    }
                    _ => {}
                }
            }
            card.event_time = if c.number == w.current_cycle().number {
                w.events
                    .last()
                    .and_then(|e| e["at"].as_u64())
                    .unwrap_or(w.created_at)
            } else {
                w.created_at
            };
            cards.push(card.clone());
            if let Some(id) = &c.run_id
                && setup::safe_path(root, &format!("{CONFIG}/runs/{id}/run.json"))?.exists()
            {
                let r = adapters::load_run(root, id)?;
                card.record_key = format!("{}:{}:feature:{id}", w.project_id, w.id);
                card.kind = "Feature".into();
                card.name = r.proposal.title.clone();
                card.stage = stage_name(r.stage).into();
                card.status = match r.stage {
                    Stage::AwaitingBuildApproval => "Needs approval",
                    Stage::AwaitingAcceptance => "Ready for review",
                    Stage::Accepted => "Done",
                    Stage::Blocked => "Blocked",
                    Stage::Cancelled => "Cancelled",
                    _ => "In progress",
                }
                .into();
                card.agent = match r.stage {
                    Stage::Planning => "planner",
                    Stage::Implementing => "implementer",
                    Stage::Reviewing => "reviewer",
                    Stage::Checking => "checks",
                    _ => "none",
                }
                .into();
                if r.active_dispatch.is_none() && r.stage != Stage::Checking {
                    card.agent = "none".into();
                }
                card.iteration = r.iteration;
                card.event_time = r
                    .events
                    .last()
                    .map(|event| event.at)
                    .unwrap_or(r.created_at);
                card.summary = r
                    .current()
                    .review
                    .as_ref()
                    .map(|review| review.summary.clone())
                    .or_else(|| {
                        r.current()
                            .implementation
                            .as_ref()
                            .map(|implementation| implementation.summary.clone())
                    })
                    .or_else(|| r.current().plan.as_ref().map(|plan| plan.summary.clone()))
                    .unwrap_or_else(|| r.proposal.scope.clone());
                if !r.current().checks.is_empty() {
                    card.summary.push_str(&format!(
                        " | Checks: {}",
                        r.current()
                            .checks
                            .iter()
                            .map(|check| format!("{} {:?}", check.id, check.outcome))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                card.blocker = r.blocked_reason.clone().unwrap_or_default();
                card.revision = r
                    .candidate_revision()
                    .unwrap_or(&r.proposal.revision)
                    .into();
                card.next_action=match r.stage{Stage::AwaitingBuildApproval=>"Review the proposal and record a build decision in the configured review source",Stage::AwaitingAcceptance=>"Inspect evidence and record acceptance or request changes",Stage::Blocked=>"Resolve the recorded cause, then resume",_=>""}.into();
                card.review_url = review_url(root, id);
                cards.push(card.clone());
            }
            let path = setup::safe_path(
                root,
                &format!("{CONFIG}/retrospectives/{}.json", c.retrospective_id),
            )?;
            if path.exists()
                || c.retrospective_summary.is_some()
                || (c.number == w.current_cycle().number
                    && matches!(
                        w.state,
                        workflow::WorkflowState::Retrospective
                            | workflow::WorkflowState::AwaitingAdoption
                    ))
            {
                card.record_key =
                    format!("{}:{}:harness:{}", w.project_id, w.id, c.retrospective_id);
                card.kind = "Harness improvement".into();
                card.name = format!("Cycle {} retrospective", c.number);
                card.stage = "Retrospective".into();
                card.status = "In progress".into();
                card.agent = "harness-reviewer".into();
                card.summary = c.retrospective_summary.clone().unwrap_or_default();
                card.next_action = String::new();
                card.blocker = String::new();
                card.review_url = String::new();
                card.revision = String::new();
                if let Some(id) = &c.learning_id {
                    let l = crate::learning::read(root, id)?;
                    if let Ok(bytes) = fs::read(setup::safe_path(
                        root,
                        &format!("{CONFIG}/review-links/{id}.json"),
                    )?) && let Ok(link) = serde_json::from_slice::<Value>(&bytes)
                    {
                        card.review_url = link["reference"]
                            .as_str()
                            .filter(|url| {
                                url.starts_with("https://www.notion.so/")
                                    || url.starts_with("https://notion.so/")
                            })
                            .unwrap_or("")
                            .into();
                    }
                    card.revision = l["revision"].as_str().unwrap_or("").into();
                    card.status = match l["disposition"].as_str() {
                        Some("adoption_authorized") => "Done",
                        Some("adoption_deferred") => "Deferred",
                        _ => "Needs approval",
                    }
                    .into();
                    card.stage = l["disposition"].as_str().unwrap_or("candidate").into();
                    card.agent = "none".into();
                    if card.status == "Needs approval" {
                        card.next_action =
                            "Review the comparison and authorize adoption or defer".into();
                    }
                } else if c.retrospective_summary.is_some() {
                    card.status = "Done".into();
                    card.stage = "No harness change".into();
                    card.agent = "none".into();
                }
                if c.number == w.current_cycle().number
                    && w.state == workflow::WorkflowState::Blocked
                {
                    card.status = "Blocked".into();
                    card.blocker = w.blocked_reason.clone().unwrap_or_default();
                }
                cards.push(card);
            }
        }
    }
    Ok(cards)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub card: Card,
    pub sequence: u64,
    pub sent_sequence: u64,
    pub attempted: bool,
    pub reference: Option<String>,
    #[serde(default)]
    pub retry_at: u64,
    #[serde(default)]
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncState {
    pub entries: BTreeMap<String, Entry>,
    pub last_success: Option<u64>,
    pub last_attempt: Option<u64>,
    pub error: Option<String>,
    pub retry_at: u64,
}
impl SyncState {
    pub fn pending(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.sequence > e.sent_sequence)
            .count()
    }
}
fn config(root: &Path) -> Result<(Config, CommandSpec, u64)> {
    let p = setup::load_profile(root)?;
    ensure!(
        setup::load_lock(root)?["profile_sha256"] == setup::digest(setup::json(&p)?.as_bytes()),
        "Configuration changed outside reviewed setup; restore or reconcile it before syncing"
    );
    let c = p.kanban.context("Notion board is not configured")?;
    c.validate()?;
    let command = p
        .review
        .command
        .context("Configure the review bridge first")?;
    Ok((c, command, p.command_timeout_seconds))
}
fn state_path(root: &Path, c: &Config) -> Result<std::path::PathBuf> {
    setup::safe_path(
        root,
        &format!(
            "{CONFIG}/kanban/{}.json",
            setup::digest(setup::json(c)?.as_bytes())
        ),
    )
}
pub fn state(root: &Path) -> Result<SyncState> {
    let (c, _, _) = config(root)?;
    let path = state_path(root, &c)?;
    if path.exists() {
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    } else {
        Ok(SyncState::default())
    }
}
pub fn probe(root: &Path) -> Result<Value> {
    let (c, command, timeout) = config(root)?;
    let response = adapters::invoke(
        &command,
        root,
        &json!({"operation":"kanban_capabilities","protocol_version":1,"config":c}),
        timeout,
    )?;
    ensure!(
        response["ready"] == true,
        "Notion board schema or permissions are not ready"
    );
    Ok(response)
}
pub fn sync(root: &Path) -> Result<SyncState> {
    let (c, command, timeout) = config(root)?;
    ensure!(
        c.enabled,
        "Notion board sync is disabled; enable it in reviewed setup first"
    );
    let dir = setup::safe_path(root, &format!("{CONFIG}/kanban"))?;
    fs::create_dir_all(&dir)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(setup::safe_path(root, &format!("{CONFIG}/kanban/.lock"))?)?;
    lock.try_lock_exclusive()
        .context("Another board synchronization is active")?;
    let path = state_path(root, &c)?;
    let mut state = state(root)?;
    for card in cards(root)? {
        let key = card.record_key.clone();
        if let Some(old) = state.entries.get_mut(&key) {
            if old.card != card {
                old.card = card;
                old.sequence += 1;
            }
        } else {
            state.entries.insert(
                key,
                Entry {
                    card,
                    sequence: 1,
                    sent_sequence: 0,
                    attempted: false,
                    reference: None,
                    retry_at: 0,
                    error: None,
                },
            );
        }
    }
    setup::atomic(&path, setup::json(&state)?.as_bytes())?;
    if setup::timestamp() < state.retry_at {
        return Ok(state);
    }
    let keys: Vec<_> = state
        .entries
        .iter()
        .filter(|(_, e)| e.sequence > e.sent_sequence && setup::timestamp() >= e.retry_at)
        .map(|(k, _)| k.clone())
        .take(8)
        .collect();
    let pass_started = std::time::Instant::now();
    for key in keys {
        if pass_started.elapsed().as_secs() >= 60 {
            break;
        }
        let command_seconds = timeout.min(60 - pass_started.elapsed().as_secs()).max(1);
        let e = state.entries[&key].clone();
        state.entries.get_mut(&key).unwrap().attempted = true;
        state.last_attempt = Some(setup::timestamp());
        setup::atomic(&path, setup::json(&state)?.as_bytes())?;
        let result = adapters::invoke(
            &command,
            root,
            &json!({"operation":"sync_card","protocol_version":1,"config":c,"card":e.card,"sequence":e.sequence,"may_create":!e.attempted,"reference":e.reference}),
            command_seconds,
        );
        match result {
            Ok(v)
                if v["status"] == "completed"
                    && v["record_key"] == key
                    && v["sequence"] == e.sequence =>
            {
                let item = state.entries.get_mut(&key).unwrap();
                item.sent_sequence = e.sequence;
                item.reference = v["reference"].as_str().map(str::to_owned);
                state.last_success = Some(setup::timestamp());
                item.error = None;
                item.retry_at = 0;
            }
            other => {
                let mut delay = 60;
                let mut limited = false;
                let reason = match other {
                    Err(error) => error.to_string(),
                    Ok(response) => {
                        if response["no_create_attempt"] == true && e.reference.is_none() {
                            state.entries.get_mut(&key).unwrap().attempted = e.attempted;
                        }
                        delay = response["retry_after"].as_u64().unwrap_or(60).max(60);
                        limited = response["rate_limited"] == true;
                        response["error"]
                            .as_str()
                            .unwrap_or("Board result identity mismatch")
                            .into()
                    }
                };
                let item = state.entries.get_mut(&key).unwrap();
                item.error = Some(format!(
                    "{}: {reason}. Retry queries the same record; uncertain creation never inserts another card.",
                    e.card.name
                ));
                item.retry_at = setup::timestamp() + delay;
                if limited {
                    state.retry_at = item.retry_at;
                }
                state.error = state.entries.values().find_map(|entry| entry.error.clone());
                setup::atomic(&path, setup::json(&state)?.as_bytes())?;
                if limited {
                    return Ok(state);
                }
            }
        }
        state.error = state.entries.values().find_map(|entry| entry.error.clone());
        setup::atomic(&path, setup::json(&state)?.as_bytes())?;
    }
    Ok(state)
}
