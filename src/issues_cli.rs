use crate::{
    adapters, issues, setup,
    workflow::{self, WorkflowState},
};
use anyhow::{Result, ensure};
use clap::Subcommand;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Debug, Subcommand)]
pub enum IssuesCommand {
    /// Read all open GitHub issues into local tasks; no agents are dispatched.
    Scan {
        #[arg(long)]
        repo: String,
        /// Only queue issues carrying this exact label. Omit to include all open issues.
        #[arg(long)]
        label: Option<String>,
    },
    /// Inspect issue tasks, source changes, workflow IDs and verified outcomes.
    Status,
    /// Advance one saved transition, preserving the workflow's approval gates.
    Step,
    /// Continuously scan issues and run planning, implementation, checks and review.
    Run {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        label: Option<String>,
        /// Explicitly synchronize review packets and poll human decisions.
        #[arg(long)]
        review: bool,
        /// Return when idle, awaiting human input, or blocked.
        #[arg(long)]
        until_wait: bool,
        #[arg(long, default_value_t=30, value_parser=clap::value_parser!(u64).range(1..=60))]
        poll_seconds: u64,
    },
    /// Requeue a finished/stopped issue with a fresh source snapshot and fresh approvals.
    Retry { number: u64 },
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", setup::json(value)?);
    Ok(())
}
pub fn execute(root: &Path, command: IssuesCommand) -> Result<()> {
    match command {
        IssuesCommand::Scan { repo, label } => {
            issues::scan(root, &repo, label.as_deref())?;
            print(&issues::status(root)?)?;
        }
        IssuesCommand::Status => print(&issues::status(root)?)?,
        IssuesCommand::Step => {
            issues::advance(root)?;
            print(&issues::status(root)?)?;
        }
        IssuesCommand::Retry { number } => {
            issues::retry(root, number)?;
            print(&issues::status(root)?)?;
        }
        IssuesCommand::Run {
            repo,
            label,
            review,
            until_wait,
            poll_seconds,
        } => {
            let interval = Duration::from_secs(poll_seconds);
            let mut last_scan = None;
            let mut last_status = String::new();
            let mut last_review = String::new();
            loop {
                if last_scan.is_none_or(|at: Instant| at.elapsed() >= interval) {
                    issues::scan(root, &repo, label.as_deref())?;
                    last_scan = Some(Instant::now());
                }
                let mut child = issues::advance(root)?;
                if review && let Some(state) = &child {
                    match state.state {
                        WorkflowState::AwaitingBuildApproval
                        | WorkflowState::AwaitingAcceptance => {
                            let run_id = state
                                .current_cycle()
                                .run_id
                                .as_ref()
                                .expect("feature gate has run");
                            let run = adapters::load_run(root, run_id)?;
                            let key = format!("{run_id}:{:?}:{}", run.stage, run.iteration);
                            if last_review != key {
                                adapters::sync_review(root, run_id)?;
                                last_review = key;
                            }
                            adapters::poll_feature_decisions(root, run_id)?;
                            child = Some(workflow::advance(root, &state.id)?);
                        }
                        WorkflowState::AwaitingAdoption => {
                            let id = state
                                .current_cycle()
                                .learning_id
                                .as_ref()
                                .expect("adoption has candidate");
                            let key = format!("harness:{id}");
                            if last_review != key {
                                adapters::sync_harness_review(root, id)?;
                                last_review = key;
                            }
                            adapters::poll_harness_decisions(root, id)?;
                            child = Some(workflow::advance(root, &state.id)?);
                        }
                        _ => {}
                    }
                }
                let status = issues::status(root)?;
                // Scan timestamps change each poll; print only changes to tasks/errors.
                let summary = serde_json::to_string(&(&status.tasks, &status.last_scan_error))?;
                if summary != last_status {
                    print(&status)?;
                    last_status = summary;
                }
                ensure!(
                    !child
                        .as_ref()
                        .is_some_and(|s| s.state == WorkflowState::Blocked),
                    "Issue workflow blocked; resolve its recorded reason, then use workflow resume WORKFLOW_ID"
                );
                let waiting = child.as_ref().is_none_or(|s| s.is_waiting());
                if waiting {
                    if until_wait {
                        break;
                    }
                    std::thread::sleep(interval);
                }
            }
        }
    }
    Ok(())
}
