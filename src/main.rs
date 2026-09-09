use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use softwarefactory::{
    adapters,
    engine::{DecisionClaim, Proposal},
    setup::{self, Profile},
    tui, versions,
};
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    version,
    about = "A project-scoped product workflow, with a safe terminal setup wizard"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".")]
    project: PathBuf,
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Subcommand)]
enum Commands {
    /// Coordinate the entire product workflow across repeated cycles.
    Workflow {
        #[command(subcommand)]
        command: softwarefactory::workflow_cli::WorkflowCommand,
    },
    /// Open the workflow control panel, or just its setup wizard.
    Tui {
        #[arg(long)]
        setup: bool,
    },
    /// Inspect local board synchronization or explicitly retry configured board writes.
    Board {
        #[arg(long)]
        sync: bool,
        #[arg(long)]
        probe: bool,
    },
    /// Print a profile template; fill in commands, reviewers and checks.
    Template {
        #[arg(default_value = "generic")]
        name: String,
        #[arg(long, default_value = "my-project")]
        id: String,
    },
    /// Preview configuration changes. --apply confirms this exact profile.
    Setup {
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        apply: bool,
    },
    /// Inspect non-mutating readiness; --probe explicitly executes adapter probes.
    Doctor {
        #[arg(long)]
        probe: bool,
    },
    /// Complete or roll back an interrupted configuration transaction.
    SetupRecover {
        #[arg(long)]
        rollback: bool,
    },
    /// Preview reverting a completed setup transaction.
    Rollback {
        transaction: String,
        #[arg(long)]
        apply: bool,
    },
    /// Preview removal of installer-owned configuration, preserving run history.
    Detach {
        #[arg(long)]
        apply: bool,
    },
    /// Save an immutable feature proposal; no agents are dispatched.
    Propose { file: PathBuf },
    /// Show a saved run and its evidence, decisions, and pending work.
    Status { run: String },
    /// Verify an attributable decision through the project's review bridge.
    Decision { run: String, file: PathBuf },
    /// Execute one authorised role/check stage, or recover its saved dispatch.
    Step { run: String },
    /// Keep advancing an authorised run until review, blocking or budget limit.
    Run { run: String },
    /// Cancel an active runtime job and preserve the run's history.
    Cancel {
        run: String,
        #[arg(long, default_value = "Cancelled by operator")]
        reason: String,
    },
    /// Explicitly synchronise an immutable review packet to the configured surface.
    SyncReview { run: String },
    /// Add optional feedback. Feedback never grants approval.
    Feedback {
        run: String,
        #[arg(long)]
        stage: String,
        #[arg(long)]
        revision: String,
        text: String,
    },
    /// Record a focused discovery question; produces no build authority.
    Discover { question: String },
    /// Advance research, opportunity synthesis or product planning in a fresh context.
    DiscoveryStep { id: String },
    /// Record a frozen harness comparison, no-change finding or deferral.
    Learn { run: String, file: PathBuf },
    /// Inspect a recorded harness comparison.
    Learning { id: Option<String> },
    /// Verify human adoption of an exact evaluated harness candidate.
    Adopt { id: String, file: PathBuf },
    /// Verify a human decision to defer an evaluated harness change.
    DeferHarness { id: String, file: PathBuf },
    /// Print the canonical source revision used by agent and check bridges.
    Snapshot,
    /// List installed releases and the active launcher version.
    Versions {
        #[arg(long)]
        prefix: Option<PathBuf>,
    },
    /// Check published releases; --apply downloads, verifies and activates an update.
    Update {
        #[arg(long)]
        prefix: Option<PathBuf>,
        /// Exact installed or published version; also supports explicit rollback.
        #[arg(long)]
        to: Option<String>,
        /// Check for a newer release without installing it (the default).
        #[arg(long, conflicts_with = "apply")]
        check: bool,
        /// Install and activate this locally built binary instead of downloading.
        #[arg(long, conflicts_with = "to")]
        local: bool,
        #[arg(long)]
        apply: bool,
    },
    /// Preview adopting this binary's version in the selected project's configuration.
    ProjectUpdate {
        #[arg(long)]
        apply: bool,
    },
    /// Install this binary as a versioned user-local release, under an explicit prefix.
    Install {
        #[arg(long)]
        prefix: PathBuf,
        #[arg(long)]
        apply: bool,
    },
}
fn read<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    Ok(serde_json::from_str(
        &std::fs::read_to_string(path)
            .with_context(|| format!("Cannot read {}", path.display()))?,
    )?)
}
fn print(v: &impl serde::Serialize) -> Result<()> {
    println!("{}", setup::json(v)?);
    Ok(())
}
fn main() {
    if let Err(e) = execute() {
        eprintln!("Software Factory: {e:#}");
        std::process::exit(1);
    }
}
fn execute() -> Result<()> {
    let cli = Cli::parse();
    // Machine-level version commands also work outside an existing project.
    let machine_command = matches!(
        &cli.command,
        Some(Commands::Install { .. } | Commands::Update { .. } | Commands::Versions { .. })
    );
    let root = if machine_command {
        cli.project.clone()
    } else {
        std::fs::canonicalize(&cli.project).context("Project directory does not exist")?
    };
    match cli.command.unwrap_or(Commands::Tui { setup: false }) {
        Commands::Workflow { command } => softwarefactory::workflow_cli::execute(&root, command)?,
        Commands::Tui { setup: false } => tui::run(&root)?,
        Commands::Tui { setup: true } => {
            softwarefactory::setup_tui::run(&root)?;
        }
        Commands::Board { sync, probe } => {
            if probe {
                print(&softwarefactory::kanban::probe(&root)?)?;
            }
            if sync {
                let state = softwarefactory::kanban::sync(&root)?;
                print(&state)?;
                anyhow::ensure!(
                    state.error.is_none(),
                    "Notion synchronization is pending; inspect board status and resolve the recorded cause"
                );
            }
            if !sync && !probe {
                print(&softwarefactory::kanban::state(&root)?)?;
            }
        }
        Commands::Template { name, id } => {
            let p = Profile::template(&name, &id);
            p.validate()?;
            print(&p)?;
        }
        Commands::Setup { profile, apply } => {
            let p: Profile = read(&profile)?;
            let plan = setup::preview(&root, &p)?;
            print(&plan)?;
            if apply {
                setup::apply(&plan)?;
                print(&setup::readiness(&root, &p))?;
            }
        }
        Commands::Doctor { probe } => {
            let p = setup::load_profile(&root)?;
            print(&setup::readiness(&root, &p))?;
            if probe {
                print(&adapters::probe(&root, &p)?)?;
                if p.kanban.is_some() {
                    print(&softwarefactory::kanban::probe(&root)?)?;
                }
            }
        }
        Commands::SetupRecover { rollback } => println!(
            "Recovered {} transaction(s)",
            setup::recover(&root, rollback)?
        ),
        Commands::Rollback { transaction, apply } => {
            let plan = setup::rollback_preview(&root, &transaction)?;
            print(&plan)?;
            if apply {
                setup::apply(&plan)?;
            }
        }
        Commands::Detach { apply } => {
            let plan = setup::detach_preview(&root)?;
            print(&plan)?;
            if apply {
                setup::apply(&plan)?;
            }
        }
        Commands::Propose { file } => print(&adapters::new_run(&root, read::<Proposal>(&file)?)?)?,
        Commands::Status { run } => print(&adapters::load_run(&root, &run)?)?,
        Commands::Decision { run, file } => print(&adapters::decision(
            &root,
            &run,
            read::<DecisionClaim>(&file)?,
        )?)?,
        Commands::Step { run } => print(&adapters::advance(&root, &run)?)?,
        Commands::Run { run } => loop {
            let r = adapters::advance(&root, &run)?;
            println!("{} · iteration {} · {:?}", r.id, r.iteration, r.stage);
            if !matches!(
                r.stage,
                softwarefactory::engine::Stage::Planning
                    | softwarefactory::engine::Stage::Implementing
                    | softwarefactory::engine::Stage::Checking
                    | softwarefactory::engine::Stage::Reviewing
            ) {
                print(&r)?;
                break;
            }
        },
        Commands::Cancel { run, reason } => print(&adapters::cancel(&root, &run, reason)?)?,
        Commands::SyncReview { run } => print(&adapters::sync_review(&root, &run)?)?,
        Commands::Feedback {
            run,
            stage,
            revision,
            text,
        } => println!(
            "{}",
            adapters::feedback(&root, &run, &stage, &revision, &text)?.display()
        ),
        Commands::Discover { question } => print(&adapters::discovery_start(&root, &question)?)?,
        Commands::DiscoveryStep { id } => print(&adapters::discovery_step(&root, &id)?)?,
        Commands::Learn { run, file } => print(&softwarefactory::learning::create(
            &root,
            &run,
            read::<serde_json::Value>(&file)?,
        )?)?,
        Commands::Learning { id } => {
            if let Some(id) = id {
                print(&softwarefactory::learning::read(&root, &id)?)?;
            } else {
                print(&softwarefactory::learning::list(&root)?)?;
            }
        }
        Commands::Adopt { id, file } => print(&adapters::adopt_learning(
            &root,
            &id,
            read::<DecisionClaim>(&file)?,
        )?)?,
        Commands::DeferHarness { id, file } => print(&adapters::defer_learning(
            &root,
            &id,
            read::<DecisionClaim>(&file)?,
        )?)?,
        Commands::Snapshot => print(&adapters::source_snapshot(&root)?)?,
        Commands::Versions { prefix } => {
            let prefix = prefix.map(Ok).unwrap_or_else(versions::default_prefix)?;
            print(&versions::list(&prefix)?)?;
        }
        Commands::Install { prefix, apply } => {
            let plan = versions::preview(&prefix, None, false)?;
            print(&plan)?;
            if apply {
                versions::apply(&plan)?;
                println!(
                    "Installed {}. Add {} to PATH.",
                    setup::VERSION,
                    plan.prefix.join("bin").display()
                );
            }
        }
        Commands::Update {
            prefix,
            to,
            check: _,
            local,
            apply,
        } => {
            let prefix = prefix.map(Ok).unwrap_or_else(versions::default_prefix)?;
            let installed = if let Some(to) = &to {
                versions::list(&prefix)?
                    .releases
                    .iter()
                    .any(|release| &release.version == to)
            } else {
                false
            };
            if local || installed {
                let plan = versions::preview(&prefix, to.as_deref(), true)?;
                print(&plan)?;
                if apply {
                    versions::apply(&plan)?;
                    println!(
                        "Active release: {}. Project pins are preserved.",
                        plan.to_version
                    );
                }
            } else {
                eprintln!("Checking published Software Factory releases...");
                let update = versions::remote::check(&prefix, to.as_deref())?;
                print(&update)?;
                if !update.update_available {
                    println!("Already up to date; no newer release is available.");
                } else if apply {
                    eprintln!(
                        "Downloading and verifying Software Factory {}...",
                        update.to_version
                    );
                    versions::remote::apply(&update)?;
                    println!(
                        "Updated to {}. Project pins are preserved; use project-update to adopt this release in a project.",
                        update.to_version
                    );
                } else {
                    println!(
                        "Update available: {} -> {}. Run the same command with --apply to install it.",
                        update.from_version, update.to_version
                    );
                }
            }
        }
        Commands::ProjectUpdate { apply } => {
            let plan = setup::update_preview(&root)?;
            print(&plan)?;
            if apply {
                setup::apply(&plan)?;
                println!(
                    "Project configured for {}. Existing runs retain their original release pins.",
                    setup::VERSION
                );
            }
        }
    }
    Ok(())
}
