use crate::{
    engine::*,
    setup::{self, *},
};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub fn templates() -> Vec<(&'static str, &'static str)> {
    vec![
        ("research", include_str!("../prompts/research.md")),
        ("opportunities", include_str!("../prompts/opportunities.md")),
        (
            "product-manager",
            include_str!("../prompts/product-manager.md"),
        ),
        ("planner", include_str!("../prompts/planner.md")),
        ("implementer", include_str!("../prompts/implementer.md")),
        ("reviewer", include_str!("../prompts/reviewer.md")),
        (
            "harness-reviewer",
            include_str!("../prompts/harness-reviewer.md"),
        ),
    ]
}

/// A command is an explicitly configured trust boundary. JSON goes over stdin;
/// stdout is one JSON response. No shell interpolation is performed.
pub fn invoke(spec: &CommandSpec, root: &Path, request: &Value, seconds: u64) -> Result<Value> {
    ensure!(
        !spec.program.is_empty(),
        "Adapter command is not configured"
    );
    if !matches!(
        request["operation"].as_str(),
        Some("cancel" | "cancel_check")
    ) && let Some(id) = request.get("run_id").and_then(Value::as_str)
    {
        ensure!(
            !run_dir(root, id)?.join("cancel-request.json").exists(),
            "Cancellation pending; no new work can start"
        );
    }
    let tmp = std::env::temp_dir().join(format!("softwarefactory-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&tmp)?;
    let result = (|| -> Result<Value> {
        let input = tmp.join("input");
        let output = tmp.join("output");
        let error = tmp.join("error");
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&input)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o700))?;
        }
        f.write_all(serde_json::to_string(request)?.as_bytes())?;
        drop(f);
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .current_dir(root)
            .stdin(File::open(input)?)
            .stdout(File::create(&output)?)
            .stderr(File::create(&error)?);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .context("Adapter unavailable: failed to start configured command")?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            let cancelled = request["operation"] != "cancel"
                && request["operation"] != "cancel_check"
                && request
                    .get("run_id")
                    .and_then(Value::as_str)
                    .filter(|id| uuid::Uuid::parse_str(id).is_ok())
                    .is_some_and(|id| {
                        root.join(CONFIG)
                            .join("runs")
                            .join(id)
                            .join("cancel-request.json")
                            .exists()
                    });
            if cancelled || started.elapsed() >= Duration::from_secs(seconds) {
                #[cfg(unix)]
                {
                    let _ = Command::new("/bin/kill")
                        .arg("-TERM")
                        .arg("--")
                        .arg(format!("-{}", child.id()))
                        .status();
                }
                let _ = child.kill();
                let _ = child.wait();
                bail!(
                    "Adapter cancelled or timed out after {seconds}s; dispatch state requires reconciliation"
                );
            }
            if fs::metadata(&output).is_ok_and(|m| m.len() > 16 * 1024 * 1024)
                || fs::metadata(&error).is_ok_and(|m| m.len() > 16 * 1024 * 1024)
            {
                let _ = child.kill();
                let _ = child.wait();
                bail!("Adapter exceeded 16 MiB output limit");
            }
            std::thread::sleep(Duration::from_millis(30));
        };
        ensure!(
            status.success(),
            "Adapter exited with {status}; stderr withheld to avoid exposing credentials"
        );
        let mut bytes = String::new();
        File::open(output)?
            .take(16 * 1024 * 1024)
            .read_to_string(&mut bytes)?;
        serde_json::from_str(&bytes).context("Adapter must emit one JSON response on stdout")
    })();
    let _ = fs::remove_dir_all(tmp);
    result
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub protocol_version: u32,
    pub roles: Vec<String>,
    pub high_reasoning_models: Vec<String>,
    pub idempotent_dispatch: bool,
    pub lookup: bool,
    pub cancel: bool,
    pub enforces_read_only: bool,
    pub enforces_scope: bool,
}
pub fn probe(root: &Path, p: &Profile) -> Result<Value> {
    let raw = invoke(
        &p.runtime.command,
        root,
        &json!({"operation":"capabilities","protocol_version":1}),
        p.command_timeout_seconds,
    )?;
    let c: Capabilities = serde_json::from_value(raw.clone())?;
    ensure!(
        c.protocol_version == 1
            && c.idempotent_dispatch
            && c.lookup
            && c.cancel
            && c.enforces_read_only
            && c.enforces_scope,
        "Runtime must enforce scope/read-only permissions and support idempotent dispatch, lookup and cancellation"
    );
    for role in ["planner", "implementer", "reviewer"] {
        ensure!(
            c.roles.iter().any(|r| r == role),
            "Runtime missing role {role}"
        );
    }
    ensure!(
        c.high_reasoning_models.contains(&p.runtime.planner_model),
        "Selected planner model lacks verified high reasoning capability"
    );
    let review = p.review.command.as_ref().context("No review command")?;
    let verified = invoke(
        review,
        root,
        &json!({"operation":"capabilities","protocol_version":1,"kind":p.review.kind}),
        p.command_timeout_seconds,
    )?;
    ensure!(
        verified["protocol_version"] == 1 && verified["attributable_decisions"] == true,
        "Review bridge cannot establish attributable decisions"
    );
    Ok(json!({"runtime":raw,"review":verified}))
}

pub fn source_snapshot(root: &Path) -> Result<Value> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
        let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            let rel = p.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
            if matches!(
                e.file_name().to_str(),
                Some(".git" | ".product-workflow" | "target" | "node_modules")
            ) {
                continue;
            }
            let ty = e.file_type()?;
            if ty.is_symlink() {
                out.insert(rel, format!("symlink:{}", fs::read_link(p)?.display()));
            } else if ty.is_dir() {
                visit(root, &p, out)?;
            } else if ty.is_file() {
                ensure!(
                    e.metadata()?.len() <= 64 * 1024 * 1024,
                    "Large source file requires an explicit repository adapter: {rel}"
                );
                out.insert(rel, digest(&fs::read(p)?));
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files)?;
    let revision = digest(&serde_json::to_vec(&files)?);
    Ok(json!({"revision":revision,"files":files}))
}
pub fn guidance_snapshot(root: &Path) -> Result<Value> {
    let mut files = BTreeMap::new();
    fn guidance(dir: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
        for name in ["AGENTS.override.md", "AGENTS.md"] {
            let p = dir.join(name);
            if p.is_file() {
                ensure!(
                    !fs::symlink_metadata(&p)?.file_type().is_symlink(),
                    "Symlink guidance unsupported"
                );
                out.insert(p.to_string_lossy().to_string(), fs::read_to_string(p)?);
                if name == "AGENTS.override.md" {
                    break;
                }
            }
        }
        Ok(())
    }
    let ancestors: Vec<_> = root.ancestors().collect();
    for dir in ancestors.into_iter().rev() {
        guidance(dir, &mut files)?;
    }
    fn scoped(dir: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
        for e in fs::read_dir(dir)? {
            let e = e?;
            if !e.file_type()?.is_dir()
                || matches!(
                    e.file_name().to_str(),
                    Some(".git" | ".product-workflow" | "target" | "node_modules")
                )
            {
                continue;
            }
            guidance(&e.path(), out)?;
            scoped(&e.path(), out)?;
        }
        Ok(())
    }
    scoped(root, &mut files)?;
    Ok(serde_json::to_value(files)?)
}
pub fn run_dir(root: &Path, id: &str) -> Result<PathBuf> {
    ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid run ID");
    safe_path(&fs::canonicalize(root)?, &format!("{CONFIG}/runs/{id}"))
}
pub fn load_run(root: &Path, id: &str) -> Result<Run> {
    run_dir(root, id)?;
    let run = Run::load(&safe_path(root, &format!("{CONFIG}/runs/{id}/run.json"))?)?;
    ensure!(run.id == id, "Run identity mismatch");
    let current = load_profile(root)?;
    ensure!(
        run.project_id == current.project_id,
        "Run belongs to another project"
    );
    Ok(run)
}
pub fn save_run(root: &Path, run: &Run) -> Result<()> {
    run.save(&run_dir(root, &run.id)?.join("run.json"))
}
pub fn pinned_profile(run: &Run) -> Result<Profile> {
    Ok(serde_json::from_value(run.profile_snapshot.clone())?)
}
pub fn new_run(root: &Path, proposal: Proposal) -> Result<Run> {
    let _lock = ProjectLock::acquire(root)?;
    let p = load_profile(root)?;
    let lock = load_lock(root)?;
    ensure!(
        lock["core"] == VERSION,
        "Project pins another core version; use that installed release or explicitly update setup"
    );
    ensure!(
        lock["profile_sha256"] == digest(setup::json(&p)?.as_bytes()),
        "Profile changed outside reviewed setup; reconcile configuration before running"
    );
    let runs = safe_path(root, &format!("{CONFIG}/runs"))?;
    if runs.exists() {
        for e in fs::read_dir(&runs)? {
            let path = e?.path().join("run.json");
            if path.is_file() {
                let other = Run::load(&path)?;
                ensure!(
                    matches!(other.stage, Stage::Accepted | Stage::Cancelled),
                    "Another feature is active: {}",
                    other.id
                );
            }
        }
    }
    let check_ids: Vec<_> = p
        .checks
        .iter()
        .filter(|c| c.required)
        .map(|c| c.id.clone())
        .collect();
    for id in check_ids {
        ensure!(
            proposal.required_checks.contains(&id),
            "Proposal silently removes required profile check {id}"
        );
    }
    for id in &proposal.all_required_checks() {
        ensure!(
            p.checks.iter().any(|c| &c.id == id),
            "Proposal references unconfigured check {id}"
        );
    }
    let template_map: BTreeMap<_, _> = templates().into_iter().collect();
    let pins = json!({"lock":lock,"source_baseline":source_snapshot(root)?,"guidance":configured_guidance(root,&p)?,"adapter_hashes":adapter_hashes(root,&p)?,"templates":template_map,"rubrics":pilot_scenarios(&p.template),"created_at":timestamp(),"root":fs::canonicalize(root)?});
    let run = Run::new(
        p.project_id.clone(),
        proposal,
        serde_json::to_value(&p)?,
        pins,
        Budget {
            max_iterations: u32::MAX,
            max_dispatches: p.max_dispatches,
            max_elapsed_seconds: p.max_seconds,
        },
    )?;
    save_run(root, &run)?;
    Ok(run)
}
struct BridgeVerifier<'a> {
    root: &'a Path,
    profile: &'a Profile,
}
impl DecisionVerifier for BridgeVerifier<'_> {
    fn verify(&self, claim: &DecisionClaim) -> Result<()> {
        ensure!(
            self.profile.review.reviewers.contains(&claim.actor),
            "Decision actor is not an authorised reviewer"
        );
        let spec = self
            .profile
            .review
            .command
            .as_ref()
            .context("Review bridge missing")?;
        let response = invoke(
            spec,
            self.root,
            &json!({"operation":"verify_decision","protocol_version":1,"claim":claim,"kind":self.profile.review.kind}),
            self.profile.command_timeout_seconds,
        )?;
        ensure!(
            response["verified"] == true,
            "Decision provenance could not be verified"
        );
        ensure!(
            response["claim"] == serde_json::to_value(claim)?,
            "Authoritative decision differs from submitted claim"
        );
        ensure!(
            response["actor_type"] == "human",
            "Automated actors cannot approve their own work"
        );
        ensure!(
            response["source_evidence"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
            "Missing attributable source evidence"
        );
        Ok(())
    }
}
pub fn decision(root: &Path, id: &str, claim: DecisionClaim) -> Result<Run> {
    let _guard = ProjectLock::acquire(root)?;
    let mut run = load_run(root, id)?;
    let p = pinned_profile(&run)?;
    verify_adapters(root, &run, &p)?;
    run.reverify_decisions(&BridgeVerifier { root, profile: &p })?;
    if matches!(
        claim.action,
        DecisionAction::Accept | DecisionAction::Merge | DecisionAction::Release
    ) {
        verify_candidate(root, &run)?;
    }
    let verified = VerifiedDecision::verify(claim, &BridgeVerifier { root, profile: &p })?;
    run.apply_decision(verified)?;
    save_run(root, &run)?;
    Ok(run)
}
pub fn sync_review(root: &Path, id: &str) -> Result<Value> {
    let _guard = ProjectLock::acquire(root)?;
    let run = load_run(root, id)?;
    let p = pinned_profile(&run)?;
    verify_adapters(root, &run, &p)?;
    let response = invoke(
        p.review.command.as_ref().context("Review bridge missing")?,
        root,
        &json!({"operation":"submit_packet","protocol_version":1,"idempotency_key":format!("{}:{}:{:?}:{}",run.project_id,run.id,run.stage,run.iteration),"packet":run}),
        p.command_timeout_seconds,
    )?;
    atomic(
        &run_dir(root, id)?.join("review-sync.json"),
        setup::json(&response)?.as_bytes(),
    )?;
    Ok(response)
}
fn candidate(root: &Path, run: &Run) -> Result<Value> {
    let p = run_dir(root, &run.id)?.join(format!("candidate-{}.json", run.iteration));
    Ok(serde_json::from_str(
        &fs::read_to_string(p).context("No frozen candidate snapshot")?,
    )?)
}
fn verify_candidate(root: &Path, run: &Run) -> Result<()> {
    ensure!(
        candidate(root, run)? == source_snapshot(root)?,
        "Source changed after candidate freeze; require a new implementation and review"
    );
    let index = run_dir(root, &run.id)?.join(format!("evidence-{}.json", run.iteration));
    ensure!(
        index.exists()
            || !matches!(
                run.stage,
                Stage::Reviewing | Stage::AwaitingAcceptance | Stage::Accepted
            ),
        "Frozen evidence index is missing; candidate cannot be reviewed or accepted"
    );
    if index.exists() {
        let hashes: BTreeMap<String, String> = serde_json::from_str(&fs::read_to_string(index)?)?;
        let implementation = run
            .current()
            .implementation
            .as_ref()
            .context("Missing implementation")?;
        let expected: std::collections::BTreeSet<_> = run
            .current()
            .checks
            .iter()
            .flat_map(|c| c.evidence_refs.iter().cloned())
            .chain([
                implementation.diff_ref.clone(),
                implementation.guidance_diff_ref.clone(),
            ])
            .collect();
        ensure!(
            hashes
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                == expected,
            "Evidence index does not cover the recorded artifacts"
        );
        for (path, hash) in hashes {
            ensure!(
                digest(&fs::read(&path).context("Evidence artifact disappeared")?) == hash,
                "Evidence artifact changed since verification: {path}"
            );
        }
    }
    Ok(())
}
#[derive(Debug, Serialize, Deserialize)]
struct PendingDispatch {
    request: Value,
    response: Option<Value>,
}
fn result_from_response(
    root: &Path,
    run: &mut Run,
    dispatch: &Dispatch,
    response: Value,
) -> Result<()> {
    ensure!(
        response["status"] == "completed",
        "Runtime job is pending or unknown; resume later with the same key"
    );
    ensure!(
        response["context_id"] == dispatch.context_id,
        "Runtime reused or mismatched role context"
    );
    let p = pinned_profile(run)?;
    if dispatch.role == Role::Planner {
        ensure!(
            response["effective_model"] == p.runtime.planner_model
                && response["effective_reasoning"] == "high",
            "Runtime did not use the selected high-reasoning planner"
        );
    }
    let mut result: RoleResult = serde_json::from_value(response["result"].clone())?;
    match &mut result {
        RoleResult::Implementer(r) => {
            for reference in [&mut r.diff_ref, &mut r.guidance_diff_ref] {
                let path = Path::new(reference.as_str());
                let path = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    root.join(path)
                };
                let path =
                    fs::canonicalize(path).context("Missing implementation evidence artifact")?;
                ensure!(
                    path.starts_with(run_dir(root, &run.id)?) && fs::metadata(&path)?.len() > 0,
                    "Implementation evidence must be a nonempty artifact in this run directory"
                );
                *reference = path.to_string_lossy().to_string();
            }
            let snapshot = source_snapshot(root)?;
            ensure!(
                snapshot["revision"] == r.candidate_revision,
                "Implementer candidate revision differs from actual source snapshot"
            );
            let dir = run_dir(root, &run.id)?;
            atomic(
                &dir.join(format!("candidate-{}.json", run.iteration)),
                setup::json(&snapshot)?.as_bytes(),
            )?;
        }
        RoleResult::Reviewer(_) => {
            verify_candidate(root, run)?;
        }
        _ => {}
    }
    run.complete_role(&dispatch.id, result)?;
    Ok(())
}
pub fn advance(root: &Path, id: &str) -> Result<Run> {
    let _guard = ProjectLock::acquire(root)?;
    let mut run = load_run(root, id)?;
    let p = pinned_profile(&run)?;
    ensure!(
        !run_dir(root, id)?.join("cancel-request.json").exists(),
        "Cancellation pending; use cancel to reconcile termination, no work resumed"
    );
    verify_adapters(root, &run, &p)?;
    run.reverify_decisions(&BridgeVerifier { root, profile: &p })?;
    ensure!(
        run.pins["lock"]["core"] == VERSION,
        "Run requires its pinned core release"
    );
    ensure!(
        run.pins["root"] == serde_json::to_value(fs::canonicalize(root)?)?,
        "Moved project requires explicit run migration; refusing guessed workspace"
    );
    if run.stage == Stage::Blocked {
        run.resume()?;
        save_run(root, &run)?;
    }
    if run.stage == Stage::Checking {
        return execute_checks(root, run, &p);
    }
    let role = match run.stage {
        Stage::Planning => Role::Planner,
        Stage::Implementing => Role::Implementer,
        Stage::Reviewing => Role::Reviewer,
        _ => bail!(
            "No executable stage: {:?}. Inspect status and required human decision.",
            run.stage
        ),
    };
    if role == Role::Reviewer {
        verify_candidate(root, &run)?;
    }
    probe(root, &p)?;
    if run.active_dispatch.is_none() {
        let expected = if role == Role::Planner {
            if run.iteration == 1 {
                run.pins["source_baseline"].clone()
            } else {
                let path = run_dir(root, id)?.join(format!("candidate-{}.json", run.iteration - 1));
                serde_json::from_str(&fs::read_to_string(path)?)?
            }
        } else if role == Role::Implementer {
            let planner = run
                .current()
                .dispatches
                .iter()
                .rev()
                .find(|d| d.role == Role::Planner)
                .context("Missing planner handoff")?;
            let intent: PendingDispatch = serde_json::from_str(&fs::read_to_string(
                run_dir(root, id)?.join(format!("dispatch-{}.json", planner.id)),
            )?)?;
            intent.request["source_snapshot"].clone()
        } else {
            candidate(root, &run)?
        };
        if source_snapshot(root)? != expected {
            run.block("Workspace differs from approved baseline or previous handoff; restore it or submit a revised proposal".into());
            save_run(root, &run)?;
            bail!("Workspace changed outside the recorded handoff");
        }
    }
    let dispatch = if let Some(d) = run.active_dispatch.clone() {
        d
    } else {
        let result = run.begin_role(role);
        save_run(root, &run)?;
        result?
    };
    let dir = run_dir(root, id)?;
    let outbox = dir.join(format!("dispatch-{}.json", dispatch.id));
    let existed = outbox.exists();
    let role_name = match role {
        Role::Planner => "planner",
        Role::Implementer => "implementer",
        Role::Reviewer => "reviewer",
    };
    let before = source_snapshot(root)?;
    let mut intent: PendingDispatch = if existed {
        serde_json::from_str(&fs::read_to_string(&outbox)?)?
    } else {
        let request = json!({"operation":"dispatch","protocol_version":1,"project_id":run.project_id,"run_id":id,"idempotency_key":dispatch.id,"context_id":dispatch.context_id,"role":role_name,"model":if role==Role::Planner {Some(&p.runtime.planner_model)}else{None},"reasoning":if role==Role::Planner{"high"}else{"default"},"permissions":if role==Role::Implementer{"workspace_write"}else{"read_only"},"allowed_paths":p.allowed_paths,"protected_paths":[CONFIG,".git"],"root":root,"artifact_directory":dir,"input":dispatch.input,"proposal":run.proposal,"profile":run.profile_snapshot,"pins":run.pins,"guidance_current":configured_guidance(root,&p)?,"source_snapshot":before,"template":run.pins["templates"][role_name],"budget":run.budget});
        let i = PendingDispatch {
            request,
            response: None,
        };
        atomic(&outbox, setup::json(&i)?.as_bytes())?;
        i
    };
    let response = if let Some(response) = intent.response.clone() {
        Ok(response)
    } else {
        let request = if existed {
            json!({"operation":"lookup","protocol_version":1,"idempotency_key":dispatch.id,"project_id":run.project_id,"run_id":id})
        } else {
            intent.request.clone()
        };
        invoke(
            &p.runtime.command,
            root,
            &request,
            p.command_timeout_seconds,
        )
    };
    let response = match response {
        Ok(r) => r,
        Err(e) => {
            if run_dir(root, id)?.join("cancel-request.json").exists() {
                let reply = invoke(
                    &p.runtime.command,
                    root,
                    &json!({"operation":"cancel","protocol_version":1,"idempotency_key":dispatch.id}),
                    p.command_timeout_seconds,
                );
                if reply.is_ok_and(|v| v["status"] == "cancelled" || v["status"] == "completed") {
                    run.cancel("Cancelled by operator".into());
                } else {
                    run.block(
                        "Cancellation requested; runtime has not confirmed termination".into(),
                    );
                }
            } else {
                run.block(format!("Runtime state uncertain: {e}. Resume looks up the saved dispatch; it never blindly redispatches."));
            }
            save_run(root, &run)?;
            return Err(e);
        }
    };
    intent.response = if response["status"] == "completed" {
        Some(response.clone())
    } else {
        None
    };
    atomic(&outbox, setup::json(&intent)?.as_bytes())?;
    if role != Role::Implementer && source_snapshot(root)? != intent.request["source_snapshot"] {
        run.block("Read-only role changed source; runtime permission enforcement failed".into());
        save_run(root, &run)?;
        bail!("Read-only role changed source");
    }
    if let Err(e) = result_from_response(root, &mut run, &dispatch, response) {
        run.block(e.to_string());
        save_run(root, &run)?;
        return Err(e);
    }
    save_run(root, &run)?;
    Ok(run)
}
fn execute_checks(root: &Path, mut run: Run, p: &Profile) -> Result<Run> {
    verify_candidate(root, &run)?;
    let revision = candidate(root, &run)?["revision"]
        .as_str()
        .context("Missing revision")?
        .to_string();
    for check in &p.checks {
        if run.current().checks.iter().any(|c| {
            c.id == check.id
                && c.revision == revision
                && c.execution == ExecutionState::Completed
                && matches!(c.outcome, CheckOutcome::Passed | CheckOutcome::Failed)
        }) {
            continue;
        }
        let dir = run_dir(root, &run.id)?;
        let key = format!(
            "{}-{}-{}",
            run.id,
            run.iteration,
            digest(check.id.as_bytes())
        );
        let evidence = dir.join(format!("check-{key}.json"));
        let intent_path = dir.join(format!("check-intent-{key}.json"));
        let _resource = if let Some(resource) = &check.resource {
            let path = std::env::temp_dir().join(format!(
                "softwarefactory-resource-{}.lock",
                digest(resource.as_bytes())
            ));
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?;
            f.try_lock_exclusive()
                .context("Shared check resource is busy")?;
            Some(f)
        } else {
            None
        };
        let result = if evidence.exists() {
            let saved: Value = serde_json::from_str(&fs::read_to_string(&evidence)?)?;
            Ok(saved["result"].clone())
        } else {
            let request = if intent_path.exists() {
                json!({"operation":"lookup_check","protocol_version":1,"project_id":run.project_id,"run_id":run.id,"idempotency_key":key,"check_id":check.id,"revision":revision})
            } else {
                let request = json!({"operation":"check","protocol_version":1,"project_id":run.project_id,"run_id":run.id,"iteration":run.iteration,"idempotency_key":key,"check_id":check.id,"category":check.category,"criterion_ids":check.criterion_ids,"revision":revision});
                atomic(&intent_path, setup::json(&request)?.as_bytes())?;
                request
            };
            invoke(&check.command, root, &request, check.timeout_seconds)
        };
        let (execution,outcome,error,payload)=match result {Ok(v) if v["revision"]==revision && v["check_id"]==check.id && v["status"]!="unknown"=>{let outcome=match v["outcome"].as_str(){Some("passed")=>CheckOutcome::Passed,Some("failed")=>CheckOutcome::Failed,Some("blocked")=>CheckOutcome::Blocked,_=>CheckOutcome::NotRun};(ExecutionState::Completed,outcome,None,v)},Ok(_)=>(ExecutionState::Unavailable,CheckOutcome::Blocked,Some("Check submission is unknown or result identity mismatches; reconcile the adapter job".into()),json!({"status":"unknown"})),Err(e)=>(ExecutionState::Unavailable,CheckOutcome::Blocked,Some(e.to_string()),json!({"error":e.to_string()}))};
        // Only durable, completed results can be replayed. Unknown outcomes retain the intent for lookup.
        let record = json!({"check":check,"execution":execution,"outcome":outcome,"result":payload,"revision":revision,"recorded_at":timestamp()});
        if execution == ExecutionState::Completed
            && matches!(outcome, CheckOutcome::Passed | CheckOutcome::Failed)
        {
            atomic(&evidence, setup::json(&record)?.as_bytes())?;
        } else {
            atomic(
                &dir.join(format!("check-error-{key}.json")),
                setup::json(&record)?.as_bytes(),
            )?;
        }
        verify_candidate(root, &run)?;
        run.record_check(CheckResult {
            id: check.id.clone(),
            revision: revision.clone(),
            execution,
            outcome,
            evidence_refs: if evidence.exists() {
                vec![evidence.to_string_lossy().to_string()]
            } else {
                vec![]
            },
            error,
        })?;
        save_run(root, &run)?;
    }
    let result = run.finish_checks();
    if result.is_ok() {
        freeze_evidence(root, &run)?;
    }
    save_run(root, &run)?;
    result?;
    Ok(run)
}
pub fn cancel(root: &Path, id: &str, reason: String) -> Result<Run> {
    let mut snapshot = load_run(root, id)?;
    let dir = run_dir(root, id)?;
    atomic(
        &dir.join("cancel-request.json"),
        setup::json(&json!({"reason":reason,"time":timestamp()}))?.as_bytes(),
    )?;
    let _guard = match ProjectLock::acquire(root) {
        Ok(g) => g,
        Err(_) => {
            snapshot.blocked_reason = Some(
                "Cancellation requested; waiting for the active command to stop and confirm".into(),
            );
            return Ok(snapshot);
        }
    };
    let mut run = load_run(root, id)?;
    if let Some(d) = &run.active_dispatch {
        let p = pinned_profile(&run)?;
        verify_adapters(root, &run, &p)?;
        let reply = invoke(
            &p.runtime.command,
            root,
            &json!({"operation":"cancel","protocol_version":1,"idempotency_key":d.id}),
            p.command_timeout_seconds,
        )?;
        ensure!(
            reply["status"] == "cancelled" || reply["status"] == "completed",
            "Runtime cancellation not confirmed"
        );
    }
    let p = pinned_profile(&run)?;
    verify_adapters(root, &run, &p)?;
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if let Some(key) = name
            .strip_prefix("check-intent-")
            .and_then(|s| s.strip_suffix(".json"))
        {
            if dir.join(format!("check-{key}.json")).exists() {
                continue;
            }
            let request: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
            let check = p
                .checks
                .iter()
                .find(|c| request["check_id"] == c.id)
                .context("Unknown pending check")?;
            let response = invoke(
                &check.command,
                root,
                &json!({"operation":"cancel_check","protocol_version":1,"idempotency_key":key,"run_id":run.id,"check_id":check.id,"revision":request["revision"]}),
                check.timeout_seconds,
            );
            if !response.is_ok_and(|v| v["status"] == "cancelled" || v["status"] == "completed") {
                run.block(format!(
                    "Check {} termination is unconfirmed; no new feature may start",
                    check.id
                ));
                save_run(root, &run)?;
                bail!(
                    "Check termination is unknown; reconcile the check adapter before cancellation can finish"
                );
            }
        }
    }
    run.cancel(reason);
    save_run(root, &run)?;
    Ok(run)
}
pub fn feedback(
    root: &Path,
    id: &str,
    stage: &str,
    revision: &str,
    wording: &str,
) -> Result<PathBuf> {
    let _guard = ProjectLock::acquire(root)?;
    let run = load_run(root, id)?;
    let path = run_dir(root, id)?.join(format!("feedback-{}.json", uuid::Uuid::new_v4()));
    atomic(&path,setup::json(&json!({"project_id":run.project_id,"run_id":id,"stage":stage,"revision":revision,"original_wording":wording,"interpretation":null,"created_at":timestamp(),"grants_approval":false}))?.as_bytes())?;
    Ok(path)
}

fn freeze_evidence(root: &Path, run: &Run) -> Result<()> {
    let mut hashes = BTreeMap::new();
    let i = run
        .current()
        .implementation
        .as_ref()
        .context("Missing implementation")?;
    for path in run
        .current()
        .checks
        .iter()
        .flat_map(|c| &c.evidence_refs)
        .chain([&i.diff_ref, &i.guidance_diff_ref])
    {
        hashes.insert(path.clone(), digest(&fs::read(path)?));
    }
    atomic(
        &run_dir(root, &run.id)?.join(format!("evidence-{}.json", run.iteration)),
        setup::json(&hashes)?.as_bytes(),
    )
}
fn configured_guidance(root: &Path, p: &Profile) -> Result<Value> {
    let mut all = guidance_snapshot(root)?;
    for rel in p.guidance.iter().chain(&p.architecture) {
        let path = safe_path(root, rel)?;
        if path.is_file() {
            all[path.to_string_lossy().as_ref()] = Value::String(fs::read_to_string(&path)?);
        }
    }
    Ok(all)
}
fn adapter_hashes(root: &Path, p: &Profile) -> Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    let commands = std::iter::once(&p.runtime.command)
        .chain(p.review.command.iter())
        .chain(p.checks.iter().map(|c| &c.command));
    for cmd in commands {
        let executable = if cmd.program.contains(std::path::MAIN_SEPARATOR) {
            let path = Path::new(&cmd.program);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                root.join(path)
            }
        } else {
            std::env::var_os("PATH")
                .and_then(|paths| {
                    std::env::split_paths(&paths)
                        .map(|p| p.join(&cmd.program))
                        .find(|p| p.is_file())
                })
                .context("Configured adapter executable unavailable")?
        };
        let executable = fs::canonicalize(executable)?;
        hashes.insert(
            executable.to_string_lossy().to_string(),
            digest(&fs::read(executable)?),
        );
        for arg in &cmd.args {
            let path = if Path::new(arg).is_absolute() {
                PathBuf::from(arg)
            } else {
                root.join(arg)
            };
            if path.is_file() {
                let path = fs::canonicalize(path)?;
                hashes.insert(path.to_string_lossy().to_string(), digest(&fs::read(path)?));
            }
        }
    }
    Ok(hashes)
}
fn verify_adapters(root: &Path, run: &Run, p: &Profile) -> Result<()> {
    ensure!(
        run.pins["adapter_hashes"] == serde_json::to_value(adapter_hashes(root, p)?)?,
        "Trusted adapter executable or script changed; use the pinned version and independently review upgrades"
    );
    Ok(())
}
pub fn adopt_learning(root: &Path, id: &str, claim: DecisionClaim) -> Result<Value> {
    let record = crate::learning::read(root, id)?;
    let run = load_run(
        root,
        record["run_id"]
            .as_str()
            .context("Missing originating run")?,
    )?;
    let p = pinned_profile(&run)?;
    verify_adapters(root, &run, &p)?;
    crate::learning::adopt(root, id, claim, &BridgeVerifier { root, profile: &p })
}

/// Discovery remains a proposal-producing activity with no feature authority.
pub fn discovery_start(root: &Path, question: &str) -> Result<Value> {
    let _guard = ProjectLock::acquire(root)?;
    let p = load_profile(root)?;
    ensure!(p.discovery_enabled, "Discovery disabled by this profile");
    ensure!(
        !question.trim().is_empty(),
        "Supply one focused product question"
    );
    let id = uuid::Uuid::new_v4().to_string();
    let record = json!({"schema_version":1,"id":id,"project_id":p.project_id,"question":question,"profile":p,"adapter_hashes":adapter_hashes(root,&p)?,"source":source_snapshot(root)?,"guidance":configured_guidance(root,&p)?,"templates":templates().into_iter().collect::<BTreeMap<_,_>>(),"created_at":timestamp(),"next_stage":0,"outputs":[],"pending":null,"status":"ready"});
    let path = safe_path(root, &format!("{CONFIG}/discovery/{id}.json"))?;
    atomic(&path, setup::json(&record)?.as_bytes())?;
    Ok(record)
}
pub fn discovery_step(root: &Path, id: &str) -> Result<Value> {
    let _guard = ProjectLock::acquire(root)?;
    ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid discovery ID");
    let path = safe_path(root, &format!("{CONFIG}/discovery/{id}.json"))?;
    let mut record: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    let p: Profile = serde_json::from_value(record["profile"].clone())?;
    ensure!(
        load_profile(root)?.project_id == p.project_id,
        "Discovery belongs to another project"
    );
    ensure!(
        record["adapter_hashes"] == serde_json::to_value(adapter_hashes(root, &p)?)?,
        "Pinned discovery adapters changed"
    );
    ensure!(
        timestamp().saturating_sub(record["created_at"].as_u64().unwrap_or(0)) < p.max_seconds,
        "Discovery time budget exhausted"
    );
    let stage = record["next_stage"]
        .as_u64()
        .context("Invalid discovery stage")? as usize;
    let roles = ["research", "opportunities", "product-manager"];
    ensure!(
        stage < roles.len(),
        "Discovery is complete; review the product proposal separately"
    );
    ensure!(
        source_snapshot(root)? == record["source"],
        "Discovery baseline changed; restore source or start a new question"
    );
    let role = roles[stage];
    let existed = !record["pending"].is_null();
    if !existed {
        let key = uuid::Uuid::new_v4().to_string();
        record["pending"] = json!({"operation":"dispatch","protocol_version":1,"project_id":p.project_id,"idempotency_key":key,"context_id":uuid::Uuid::new_v4().to_string(),"role":role,"permissions":"read_only","protected_paths":[CONFIG,".git"],"root":root,"question":record["question"],"previous_outputs":record["outputs"],"profile":p,"guidance":record["guidance"],"template":record["templates"][role]});
        atomic(&path, setup::json(&record)?.as_bytes())?;
    }
    let request = if existed {
        json!({"operation":"lookup","protocol_version":1,"idempotency_key":record["pending"]["idempotency_key"]})
    } else {
        record["pending"].clone()
    };
    let response = invoke(
        &p.runtime.command,
        root,
        &request,
        p.command_timeout_seconds,
    )?;
    ensure!(
        source_snapshot(root)? == record["source"],
        "Read-only discovery changed source"
    );
    ensure!(
        response["status"] == "completed"
            && response["context_id"] == record["pending"]["context_id"],
        "Discovery pending or context mismatch; next step reconciles same dispatch"
    );
    let output = response["result"]["result"].clone();
    match stage {
        0 => {
            let findings = output["findings"]
                .as_array()
                .context("Research needs a findings array with sourced claims")?;
            for f in findings {
                for field in [
                    "claim",
                    "source",
                    "observed_at",
                    "confidence",
                    "limitations",
                ] {
                    ensure!(f.get(field).is_some(), "Research finding missing {field}");
                }
            }
        }
        1 => {
            ensure!(
                output["opportunities"]
                    .as_array()
                    .is_some_and(|a| a.len() <= 3),
                "Opportunity pass must return at most three opportunities"
            );
        }
        2 => {
            if let Some(proposal) = output.get("proposal") {
                serde_json::from_value::<Proposal>(proposal.clone())?.validate()?;
            } else {
                ensure!(
                    output["disposition"].as_str().is_some_and(|s| [
                        "no_action",
                        "defer",
                        "research"
                    ]
                    .contains(&s)),
                    "PM must return proposal or a reasoned disposition"
                );
            }
        }
        _ => unreachable!(),
    }
    record["outputs"]
        .as_array_mut()
        .context("Invalid discovery outputs")?
        .push(json!({"role":role,"output":output,"context_id":response["context_id"]}));
    record["pending"] = Value::Null;
    record["next_stage"] = json!(stage + 1);
    record["status"] = json!(if stage == 2 {
        "ready_for_product_review"
    } else {
        "ready"
    });
    atomic(&path, setup::json(&record)?.as_bytes())?;
    Ok(record)
}
