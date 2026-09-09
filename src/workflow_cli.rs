use crate::{
    adapters, setup,
    workflow::{self, WorkflowState},
};
use anyhow::{Result, ensure};
use clap::Subcommand;
use std::{path::Path, time::Duration};

#[derive(Debug, Subcommand)]
pub enum WorkflowCommand {
    /// Start a saved workflow covering discovery, features, review, learning and repetition.
    Start {
        question: String,
        #[arg(long)]
        max_cycles: Option<u32>,
    },
    /// Run the whole workflow continuously, waiting for human decisions within the same session.
    Run {
        id: String,
        /// Explicitly synchronize review packets and poll the configured human-decision source.
        #[arg(long)]
        review: bool,
        /// Return at the next approval/opportunity wait, useful for scripts.
        #[arg(long)]
        until_wait: bool,
        #[arg(long,default_value_t=5,value_parser=clap::value_parser!(u64).range(1..=60))]
        poll_seconds: u64,
    },
    /// Perform one saved transition of the whole workflow.
    Step { id: String },
    /// Inspect a workflow, its cycles, pending gate and linked feature/evidence IDs.
    Status { id: String },
    /// List whole-workflow sessions for this project.
    List,
    /// Resume a blocked transition or begin another discovery cycle after no-action.
    Resume { id: String },
    /// Stop the outer loop and reconcile any running child job before permitting new work.
    Stop {
        id: String,
        #[arg(long, default_value = "Stopped by operator")]
        reason: String,
    },
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", setup::json(value)?);
    Ok(())
}
pub fn execute(root: &Path, command: WorkflowCommand) -> Result<()> {
    match command {
        WorkflowCommand::Start {
            question,
            max_cycles,
        } => print(&workflow::start(root, &question, max_cycles)?)?,
        WorkflowCommand::Status { id } => print(&workflow::load(root, &id)?)?,
        WorkflowCommand::List => print(&workflow::list(root)?)?,
        WorkflowCommand::Step { id } => print(&workflow::advance(root, &id)?)?,
        WorkflowCommand::Resume { id } => print(&workflow::resume(root, &id)?)?,
        WorkflowCommand::Stop { id, reason } => print(&workflow::stop(root, &id, &reason)?)?,
        WorkflowCommand::Run {
            id,
            review,
            until_wait,
            poll_seconds,
        } => {
            let mut last_status = String::new();
            let mut last_review = String::new();
            loop {
                let mut state = workflow::advance(root, &id)?;
                let status = format!(
                    "cycle {} · {:?} · {} agent dispatches",
                    state.current_cycle().number,
                    state.state,
                    state.dispatches_used
                );
                if status != last_status {
                    println!("{status}");
                    if let Some(reason) = &state.blocked_reason {
                        println!("{reason}");
                    }
                    last_status = status;
                }
                if state.is_terminal() {
                    if crate::setup::load_profile(root)?
                        .kanban
                        .is_some_and(|config| config.enabled)
                        && let Err(error) = crate::kanban::sync(root)
                    {
                        eprintln!("Notion sync unavailable: {error}");
                    }
                    print(&state)?;
                    break;
                }
                if state.state == WorkflowState::Blocked {
                    if crate::setup::load_profile(root)?
                        .kanban
                        .is_some_and(|config| config.enabled)
                    {
                        match crate::kanban::sync(root) {
                            Ok(sync) => {
                                if let Some(error) = sync.error {
                                    eprintln!("Notion sync pending: {error}");
                                }
                            }
                            Err(error) => eprintln!("Notion sync unavailable: {error}"),
                        }
                    }
                    print(&state)?;
                    ensure!(
                        false,
                        "Whole workflow is blocked; resolve the recorded cause, then resume {id}"
                    );
                }
                if review {
                    let cycle = state.current_cycle();
                    match state.state {
                        WorkflowState::AwaitingBuildApproval
                        | WorkflowState::AwaitingAcceptance => {
                            let run_id = cycle.run_id.as_ref().expect("feature gate has a run");
                            let run = adapters::load_run(root, run_id)?;
                            let key = format!("{run_id}:{:?}:{}", run.stage, run.iteration);
                            if last_review != key {
                                adapters::sync_review(root, run_id)?;
                                last_review = key;
                            }
                            adapters::poll_feature_decisions(root, run_id)?;
                            state = workflow::load(root, &id)?;
                        }
                        WorkflowState::AwaitingAdoption => {
                            let candidate = cycle
                                .learning_id
                                .as_ref()
                                .expect("adoption gate has a candidate");
                            let key = format!("harness:{candidate}");
                            if last_review != key {
                                adapters::sync_harness_review(root, candidate)?;
                                last_review = key;
                            }
                            adapters::poll_harness_decisions(root, candidate)?;
                            state = workflow::advance(root, &id)?;
                        }
                        _ => {}
                    }
                }
                if crate::setup::load_profile(root)?
                    .kanban
                    .is_some_and(|config| config.enabled)
                {
                    match crate::kanban::sync(root) {
                        Ok(sync) => {
                            if let Some(error) = sync.error {
                                eprintln!("Notion sync pending: {error}");
                            }
                        }
                        Err(error) => eprintln!("Notion sync unavailable: {error}"),
                    }
                }
                if state.is_waiting() {
                    if until_wait {
                        print(&state)?;
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(poll_seconds));
                }
            }
        }
    }
    Ok(())
}
