use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use softwarefactory::{
    adapters,
    engine::{DecisionClaim, Proposal},
    setup::{self, Profile},
    tui,
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
    /// Open the keyboard-driven project setup wizard.
    Tui,
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
    /// Print the canonical source revision used by agent and check bridges.
    Snapshot,
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
    let root = std::fs::canonicalize(&cli.project).context("Project directory does not exist")?;
    match cli.command.unwrap_or(Commands::Tui) {
        Commands::Tui => tui::run(&root)?,
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
        Commands::Snapshot => print(&adapters::source_snapshot(&root)?)?,
        Commands::Install { prefix, apply } => {
            ensure!(prefix.is_absolute(), "Installation prefix must be absolute");
            let exe = std::env::current_exe()?;
            let dest = prefix
                .join("releases")
                .join(setup::VERSION)
                .join("softwarefactory");
            let launch = prefix.join("bin/softwarefactory");
            println!(
                "Release: {}\nLauncher: {}",
                dest.display(),
                launch.display()
            );
            if apply {
                ensure!(
                    !dest.exists(),
                    "Release already installed; existing releases are immutable"
                );
                let release = dest.parent().unwrap();
                std::fs::create_dir_all(release.parent().unwrap())?;
                let staging = release
                    .parent()
                    .unwrap()
                    .join(format!(".staging-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir(&staging)?;
                std::fs::copy(exe, staging.join("softwarefactory"))?;
                std::fs::create_dir(staging.join("bridges"))?;
                std::fs::write(
                    staging.join("bridges/notion_review.py"),
                    include_str!("../bridges/notion_review.py"),
                )?;
                std::fs::write(
                    staging.join("bridges/native_check.py"),
                    include_str!("../bridges/native_check.py"),
                )?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(
                        staging.join("softwarefactory"),
                        std::fs::Permissions::from_mode(0o755),
                    )?;
                }
                std::fs::rename(staging, release)?;
                std::fs::create_dir_all(launch.parent().unwrap())?;
                if std::fs::symlink_metadata(&launch).is_err() {
                    #[cfg(unix)]
                    {
                        std::os::unix::fs::symlink(&dest, &launch)?;
                    }
                    #[cfg(not(unix))]
                    {
                        std::fs::copy(&dest, &launch)?;
                    }
                } else {
                    println!(
                        "Existing launcher retained. Run {} to select this release.",
                        dest.display()
                    );
                }
                println!(
                    "Installed {}. Add {} to PATH.",
                    setup::VERSION,
                    launch.parent().unwrap().display()
                );
            }
        }
    }
    Ok(())
}
