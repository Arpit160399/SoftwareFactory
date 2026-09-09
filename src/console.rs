//! Shared console read model and responsive worker. No UI handler performs network work.
use crate::{
    adapters, kanban, setup,
    workflow::{self, Workflow, WorkflowState},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub name: String,
    pub status: String,
    pub detail: String,
}
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub observed_at: Option<Instant>,
    pub name: String,
    pub brief: String,
    pub workflows: Vec<Workflow>,
    pub cards: Vec<kanban::Card>,
    pub requirements: Vec<Requirement>,
    pub sync: Option<kanban::SyncState>,
    pub board_enabled: bool,
    pub problem: Option<String>,
    pub settings: String,
}
impl Snapshot {
    pub fn read(root: &Path) -> Self {
        let mut result = Self {
            observed_at: Some(Instant::now()),
            ..Self::default()
        };
        match setup::load_profile(root) {
            Err(e) => {
                result.name = "Project setup needed".into();
                result.problem = Some(e.to_string());
                result.requirements.push(Requirement {
                    name: "Project setup".into(),
                    status: "Missing".into(),
                    detail: "Press E to configure this project".into(),
                });
            }
            Ok(p) => {
                result.name = p.name.clone();
                result.brief = p.product_brief.clone();
                result.board_enabled = p.kanban.as_ref().is_some_and(|c| c.enabled);
                let ready = setup::readiness(root, &p);
                for text in ready.blockers {
                    result.requirements.push(Requirement {
                        name: "Required setup".into(),
                        status: "Missing".into(),
                        detail: text,
                    });
                }
                result.requirements.push(Requirement {
                    name: "Configuration".into(),
                    status: if ready.ready { "Ready" } else { "Missing" }.into(),
                    detail: "E edits setup; changes are previewed before applying".into(),
                });
                result.requirements.push(Requirement{name:"Agent runtime and review source".into(),status:"Not checked".into(),detail:"P verifies capabilities and connection; a raw CLI needs a compatible bridge".into()});
                result.requirements.push(Requirement {
                    name: "Verification checks".into(),
                    status: if p.checks.is_empty() {
                        "Missing"
                    } else {
                        "Not checked"
                    }
                    .into(),
                    detail: format!(
                        "{} configured; executed against an approved candidate, never during setup",
                        p.checks.len()
                    ),
                });
                result.requirements.push(Requirement{name:"Notion board".into(),status:if p.kanban.is_none(){"Missing"}else{"Not checked"}.into(),detail:if result.board_enabled{"Enabled; P validates schema/access, S synchronizes"}else{"Optional; configure database/data source and explicitly enable sync in setup"}.into()});
                let probe =
                    setup::safe_path(root, &format!("{}/console-probe.json", setup::CONFIG));
                if let Ok(path) = probe
                    && let Ok(bytes) = fs::read(path)
                    && let Ok(v) = serde_json::from_slice::<Value>(&bytes)
                    && v["profile_hash"]
                        == setup::digest(setup::json(&p).unwrap_or_default().as_bytes())
                    && setup::timestamp().saturating_sub(v["checked_at"].as_u64().unwrap_or(0))
                        < 300
                    && adapters::adapter_hashes(root, &p)
                        .ok()
                        .and_then(|hashes| serde_json::to_value(hashes).ok())
                        .is_some_and(|hashes| hashes == v["adapter_hashes"])
                {
                    for req in &mut result.requirements {
                        if req.name == "Agent runtime and review source" {
                            req.status = if v["runtime_ok"] == true {
                                "Ready"
                            } else {
                                "Failed"
                            }
                            .into();
                            req.detail = v["runtime_detail"]
                                .as_str()
                                .unwrap_or("Probe unavailable")
                                .into();
                        }
                        if req.name == "Notion board" && p.kanban.is_some() {
                            req.status = if v["board_ok"] == true {
                                "Ready"
                            } else {
                                "Failed"
                            }
                            .into();
                            req.detail = v["board_detail"]
                                .as_str()
                                .unwrap_or("Probe unavailable")
                                .into();
                        }
                    }
                }
                result.settings = format!(
                    "Project: {}\nLocation: {}\nCore: {}\n\nWhole-workflow budget: {} minutes / {} agent calls\nCommand timeout: {} seconds\nPlanner: {} (high reasoning)\nAllowed paths: {}\nReviewers: {}\nNotion board: {}\n\nE  Preview configuration changes\nV  Inspect installed versions\nU  Check published updates (network read only)\n\nConfiguration changes apply through the existing setup service.\nActive runs retain their pinned configuration.\nSecrets are read from the launch environment, never entered here.",
                    p.project_id,
                    root.display(),
                    setup::VERSION,
                    p.max_seconds / 60,
                    p.max_dispatches,
                    p.command_timeout_seconds,
                    p.runtime.planner_model,
                    p.allowed_paths.join(", "),
                    p.review.reviewers.len(),
                    if result.board_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
                match workflow::list(root) {
                    Ok(w) => result.workflows = w,
                    Err(e) => result.problem = Some(format!("Cannot read workflow history: {e}")),
                }
                match kanban::cards(root) {
                    Ok(c) => result.cards = c,
                    Err(e) => result.problem = Some(format!("Cannot read task details: {e}")),
                }
                if p.kanban.is_some() {
                    match kanban::state(root) {
                        Ok(s) => result.sync = Some(s),
                        Err(e) => {
                            result.problem = Some(format!("Cannot read board sync state: {e}"))
                        }
                    }
                }
            }
        }
        result
    }
}
pub fn probe(root: &Path) -> Result<String> {
    let p = setup::load_profile(root)?;
    let hash = setup::digest(setup::json(&p)?.as_bytes());
    let runtime = adapters::probe(root, &p).and_then(|v| {
        let roles = v["runtime"]["roles"]
            .as_array()
            .or_else(|| v["roles"].as_array());
        if p.discovery_enabled {
            ensure!(
                roles.is_some_and(|a| [
                    "research",
                    "opportunities",
                    "product-manager",
                    "harness-reviewer"
                ]
                .iter()
                .all(|role| a.iter().any(|v| v == role))),
                "Runtime lacks the discovery or retrospective roles required by the whole loop"
            );
        }
        Ok(v)
    });
    let board = if p.kanban.is_some() {
        kanban::probe(root)
    } else {
        Ok(json!({"disabled":true}))
    };
    let runtime_detail=match &runtime{Ok(_)=>"Capabilities verified; provider authentication must be enforced by the configured bridge".into(),Err(e)=>e.to_string()};
    let board_detail = match &board {
        Ok(_) => {
            "Schema and read access verified; write access is confirmed on explicit sync".into()
        }
        Err(e) => e.to_string(),
    };
    let record = json!({"profile_hash":hash,"adapter_hashes":adapters::adapter_hashes(root,&p).ok(),"checked_at":setup::timestamp(),"runtime_ok":runtime.is_ok(),"runtime_detail":runtime_detail,"board_ok":board.is_ok(),"board_detail":board_detail});
    setup::atomic(
        &setup::safe_path(root, &format!("{}/console-probe.json", setup::CONFIG))?,
        setup::json(&record)?.as_bytes(),
    )?;
    runtime?;
    Ok(format!("Runtime: {runtime_detail}\nNotion: {board_detail}"))
}
pub fn tick(root: &Path, id: &str, review: bool) -> Result<Workflow> {
    let mut state = workflow::advance(root, id)?;
    if review {
        let cycle = state.current_cycle();
        match state.state {
            WorkflowState::AwaitingBuildApproval | WorkflowState::AwaitingAcceptance => {
                let child = cycle.run_id.as_deref().context("Missing feature")?;
                adapters::sync_review(root, child)?;
                adapters::poll_feature_decisions(root, child)?;
                state = workflow::load(root, id)?;
            }
            WorkflowState::AwaitingAdoption => {
                let child = cycle.learning_id.as_deref().context("Missing comparison")?;
                adapters::sync_harness_review(root, child)?;
                adapters::poll_harness_decisions(root, child)?;
            }
            _ => {}
        }
    }
    Ok(state)
}
#[derive(Debug)]
pub enum Action {
    Refresh,
    Probe,
    Start(String, Option<u32>),
    Run(String),
    Resume(String),
    Pause,
    Sync,
    Versions,
    UpdateCheck,
    Shutdown,
}
pub enum Event {
    Snapshot(Box<Snapshot>),
    Message(String),
    Running(Option<String>),
    Busy(bool),
    Closed,
}
pub struct Worker {
    pub tx: mpsc::Sender<Action>,
    pub rx: mpsc::Receiver<Event>,
}
pub fn worker(root: PathBuf) -> Worker {
    let (tx, commands) = mpsc::channel();
    let (events, rx) = mpsc::channel();
    let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader_closed = closed.clone();
    let reader_events = events.clone();
    let reader_root = root.clone();
    std::thread::spawn(move || {
        while !reader_closed.load(std::sync::atomic::Ordering::Acquire) {
            if reader_events
                .send(Event::Snapshot(Box::new(Snapshot::read(&reader_root))))
                .is_err()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    std::thread::spawn(move || {
        let mut active: Option<String> = None;
        let mut next = Instant::now();
        let mut refresh = Instant::now();
        let send_snapshot = || {
            let _ = events.send(Event::Snapshot(Box::new(Snapshot::read(&root))));
        };
        send_snapshot();
        loop {
            let action = commands.recv_timeout(Duration::from_millis(100));
            if let Ok(action) = action {
                if matches!(action, Action::Shutdown) {
                    closed.store(true, std::sync::atomic::Ordering::Release);
                    let _ = events.send(Event::Closed);
                    break;
                }
                let _ = events.send(Event::Busy(true));
                let result = (|| -> Result<String> {
                    match action {
                        Action::Refresh => Ok("Project refreshed".into()),
                        Action::Probe => probe(&root),
                        Action::Start(question, limit) => {
                            ensure!(active.is_none(), "Pause the active runner first");
                            let p = setup::load_profile(&root)?;
                            let ready = setup::readiness(&root, &p);
                            ensure!(
                                ready.ready,
                                "Missing prerequisites: {}",
                                ready.blockers.join("; ")
                            );
                            probe(&root)?;
                            let w = workflow::start(&root, &question, limit)?;
                            active = Some(w.id);
                            Ok("Workflow started; human decisions remain required".into())
                        }
                        Action::Run(id) => {
                            ensure!(active.is_none(), "Runner is already active");
                            let state = workflow::load(&root, &id)?;
                            ensure!(
                                !state.is_terminal(),
                                "This workflow is finished; start a new one"
                            );
                            ensure!(
                                state.state != WorkflowState::Blocked,
                                "Resolve the blocker and use Resume"
                            );
                            active = Some(id);
                            Ok("Runner active; configured review and board synchronization enabled".into())
                        }
                        Action::Resume(id) => {
                            workflow::resume(&root, &id)?;
                            active = Some(id);
                            Ok("Resumed from the saved stage".into())
                        }
                        Action::Pause => {
                            active = None;
                            Ok("Paused between stages; progress is saved".into())
                        }
                        Action::Sync => {
                            let s = kanban::sync(&root)?;
                            ensure!(s.error.is_none(), "{}", s.error.clone().unwrap_or_default());
                            Ok(format!("Notion synchronized; {} pending", s.pending()))
                        }
                        Action::Versions => {
                            let prefix = crate::versions::default_prefix()?;
                            setup::json(&crate::versions::list(&prefix)?)
                        }
                        Action::UpdateCheck => {
                            let prefix = crate::versions::default_prefix()?;
                            let plan = crate::versions::remote::check(&prefix, None)?;
                            setup::json(&plan)
                        }
                        Action::Shutdown => unreachable!(),
                    }
                })();
                let _ = events.send(Event::Message(
                    result.unwrap_or_else(|e| format!("Action failed: {e}")),
                ));
                let _ = events.send(Event::Running(active.clone()));
                let _ = events.send(Event::Busy(false));
                send_snapshot();
                next = Instant::now();
                continue;
            } else if matches!(action, Err(mpsc::RecvTimeoutError::Disconnected)) {
                closed.store(true, std::sync::atomic::Ordering::Release);
                break;
            }
            if let Some(id) = active.clone()
                && Instant::now() >= next
            {
                let _ = events.send(Event::Busy(true));
                match tick(&root, &id, true) {
                    Ok(state) => {
                        if state.is_terminal()
                            || matches!(
                                state.state,
                                WorkflowState::Blocked | WorkflowState::WaitingForOpportunity
                            )
                        {
                            active = None;
                        }
                        next = Instant::now()
                            + if state.is_waiting() {
                                Duration::from_secs(5)
                            } else {
                                Duration::from_millis(100)
                            };
                    }
                    Err(e) => {
                        active = None;
                        let _=events.send(Event::Message(format!("Runner paused: {e}. Inspect the saved stage, resolve the cause, then Run or Resume.")));
                    }
                }
                if setup::load_profile(&root)
                    .ok()
                    .and_then(|p| p.kanban)
                    .is_some_and(|c| c.enabled)
                {
                    match kanban::sync(&root) {
                        Ok(s) => {
                            if let Some(error) = s.error {
                                let _ = events
                                    .send(Event::Message(format!("Board sync pending: {error}")));
                            }
                        }
                        Err(e) => {
                            let _ =
                                events.send(Event::Message(format!("Board sync unavailable: {e}")));
                        }
                    }
                }
                let _ = events.send(Event::Running(active.clone()));
                let _ = events.send(Event::Busy(false));
                send_snapshot();
                refresh = Instant::now();
            } else if refresh.elapsed() > Duration::from_secs(2) {
                send_snapshot();
                refresh = Instant::now();
            }
        }
    });
    Worker { tx, rx }
}
