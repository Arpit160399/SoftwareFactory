use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CONFIG: &str = ".product-workflow";
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeConfig {
    pub command: CommandSpec,
    pub planner_model: String,
    pub planner_reasoning: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewConfig {
    pub kind: String,
    pub command: Option<CommandSpec>,
    pub reviewers: Vec<String>,
    pub credential_env: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Check {
    pub id: String,
    pub category: String,
    pub required: bool,
    pub command: CommandSpec,
    pub criterion_ids: Vec<String>,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub resource: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub schema_version: u32,
    pub project_id: String,
    pub name: String,
    pub product_brief: String,
    pub template: String,
    pub guidance: Vec<String>,
    pub architecture: Vec<String>,
    pub journeys: Vec<String>,
    pub runtime: RuntimeConfig,
    pub review: ReviewConfig,
    pub checks: Vec<Check>,
    pub max_seconds: u64,
    pub max_dispatches: u32,
    pub command_timeout_seconds: u64,
    pub allowed_paths: Vec<String>,
    pub discovery_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kanban: Option<crate::kanban::Config>,
}
impl Profile {
    pub fn template(name: &str, id: &str) -> Self {
        let meal = name == "meal-map";
        Self {
            schema_version: 1,
            project_id: id.into(),
            name: if meal { "Meal Map".into() } else { id.into() },
            product_brief: if meal {
                "Native iOS meal planning: useful journeys, preserved meals, honest loading and recovery. Android is outside this pilot.".into()
            } else {
                String::new()
            },
            template: name.into(),
            guidance: vec!["AGENTS.md".into()],
            architecture: if meal {
                vec!["ARCHITECTURE.md".into()]
            } else {
                vec![]
            },
            journeys: if meal {
                vec![
                    "Generate, edit, review and save a plan".into(),
                    "Add a recipe while preserving other meals".into(),
                    "Replace one meal".into(),
                    "Generate groceries for selected meals".into(),
                    "Failure and retry without duplicates".into(),
                    "Cancel without changing saved state".into(),
                    "Save and relaunch persistence".into(),
                ]
            } else {
                vec![]
            },
            runtime: RuntimeConfig {
                command: CommandSpec {
                    program: String::new(),
                    args: vec![],
                },
                planner_model: String::new(),
                planner_reasoning: "high".into(),
            },
            review: ReviewConfig {
                kind: if meal {
                    "notion".into()
                } else {
                    "command".into()
                },
                command: None,
                reviewers: vec![],
                credential_env: if meal {
                    Some("NOTION_TOKEN".into())
                } else {
                    None
                },
            },
            checks: vec![],
            max_seconds: 3600,
            max_dispatches: 30,
            command_timeout_seconds: 600,
            allowed_paths: vec![".".into()],
            discovery_enabled: true,
            kanban: None,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "Unsupported profile schema");
        ensure!(
            !self.project_id.is_empty()
                && self.project_id.len() <= 80
                && self
                    .project_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "Project ID must contain only letters, digits, hyphens or underscores"
        );
        ensure!(
            ["generic", "meal-map"].contains(&self.template.as_str()),
            "Unsupported template"
        );
        ensure!(
            ["command", "notion"].contains(&self.review.kind.as_str()),
            "Unsupported review adapter"
        );
        ensure!(
            self.runtime.planner_reasoning == "high",
            "Planner requires high reasoning; no silent fallback"
        );
        ensure!(
            self.max_seconds > 0 && self.max_dispatches >= 3 && self.command_timeout_seconds > 0,
            "Budgets must allow at least one three-role iteration"
        );
        for p in self
            .guidance
            .iter()
            .chain(&self.architecture)
            .chain(&self.allowed_paths)
        {
            relative(p)?;
        }
        if let Some(env) = &self.review.credential_env {
            ensure!(
                !env.is_empty() && env.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "Use a credential environment variable name, not its value"
            );
        }
        if let Some(config) = &self.kanban {
            config.validate()?;
        }
        let mut ids = std::collections::HashSet::new();
        for c in &self.checks {
            ensure!(
                ids.insert(&c.id) && !c.id.is_empty(),
                "Duplicate or empty check ID"
            );
            ensure!(
                ["product_quality", "delivered_behaviour", "harness_quality"]
                    .contains(&c.category.as_str()),
                "Unknown check category"
            );
            ensure!(
                !c.command.program.is_empty() && c.timeout_seconds > 0,
                "Check requires command and timeout"
            );
        }
        Ok(())
    }
}
pub fn relative(s: &str) -> Result<()> {
    let p = Path::new(s);
    ensure!(
        !s.is_empty()
            && !p.is_absolute()
            && p.components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
        "Path must remain repository-relative: {s}"
    );
    Ok(())
}
pub fn safe_path(root: &Path, rel: &str) -> Result<PathBuf> {
    relative(rel)?;
    let mut p = root.to_path_buf();
    for part in Path::new(rel).components() {
        p.push(part);
        if let Ok(m) = fs::symlink_metadata(&p) {
            ensure!(
                !m.file_type().is_symlink(),
                "Refusing symlink: {}",
                p.display()
            );
        }
    }
    Ok(p)
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Missing parent")?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".tmp-{}", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
pub fn json<T: Serialize>(v: &T) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(v)?))
}
pub fn config_root(root: &Path) -> Result<PathBuf> {
    safe_path(root, CONFIG)
}
pub struct ProjectLock(File);
impl ProjectLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        let dir = config_root(root)?;
        fs::create_dir_all(&dir)?;
        let p = safe_path(root, &format!("{CONFIG}/.lock"))?;
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(p)?;
        f.try_lock_exclusive()
            .context("Another setup or run owns this project")?;
        Ok(Self(f))
    }
}
impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Change {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupPlan {
    pub id: String,
    pub root: PathBuf,
    pub action: String,
    pub changes: Vec<Change>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    plan: SetupPlan,
    state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Ownership {
    files: BTreeMap<String, String>,
}
fn text_at(root: &Path, path: &str) -> Result<Option<String>> {
    let p = safe_path(root, path)?;
    if p.exists() {
        Ok(Some(fs::read_to_string(p)?))
    } else {
        Ok(None)
    }
}
fn pending(root: &Path) -> Result<Vec<PathBuf>> {
    let p = safe_path(root, &format!("{CONFIG}/transactions"))?;
    let mut out = vec![];
    if p.exists() {
        for entry in fs::read_dir(p)? {
            let path = entry?.path();
            ensure!(
                !fs::symlink_metadata(&path)?.file_type().is_symlink(),
                "Symlink in transactions"
            );
            if path.extension().is_some_and(|e| e == "json") {
                let j: Journal = serde_json::from_str(&fs::read_to_string(&path)?)?;
                if j.state == "pending" {
                    out.push(path);
                }
            }
        }
    }
    Ok(out)
}
pub fn load_profile(root: &Path) -> Result<Profile> {
    ensure!(
        pending(root)?.is_empty(),
        "Interrupted setup: run setup-recover first"
    );
    let profile: Profile = serde_json::from_str(
        &text_at(root, &format!("{CONFIG}/profile.json"))?
            .context("Project has no setup; run softwarefactory tui")?,
    )?;
    profile.validate()?;
    Ok(profile)
}
pub fn load_lock(root: &Path) -> Result<serde_json::Value> {
    Ok(serde_json::from_str(
        &text_at(root, &format!("{CONFIG}/lock.json"))?.context("Missing version lock")?,
    )?)
}
/// Reuse the reviewed profile and transaction journal when adopting a release.
/// Runs and other projects are outside the setup transaction's target allow-list.
pub fn update_preview(root: &Path) -> Result<SetupPlan> {
    let profile = load_profile(root)?;
    let lock = load_lock(root)?;
    ensure!(
        lock["schema_version"] == 1 && lock["adapter_protocol"] == 1,
        "Unsupported project schema or adapter protocol; no automatic migration is available"
    );
    let previous = lock["core"]
        .as_str()
        .context("Missing project core version")?;
    semver::Version::parse(previous).context("Invalid project core version")?;
    let mut plan = preview(root, &profile)?;
    plan.action = format!("update {previous} -> {VERSION}");
    Ok(plan)
}
pub fn preview(root: &Path, profile: &Profile) -> Result<SetupPlan> {
    profile.validate()?;
    let root = fs::canonicalize(root)?;
    ensure!(root.is_dir(), "Select a project directory");
    ensure!(
        pending(&root)?.is_empty(),
        "Recover interrupted setup first"
    );
    let own_path = format!("{CONFIG}/ownership.json");
    let old: Ownership = text_at(&root, &own_path)?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();
    if let Some(existing) = text_at(&root, &format!("{CONFIG}/profile.json"))? {
        let p: Profile = serde_json::from_str(&existing)?;
        ensure!(
            p.project_id == profile.project_id,
            "Project identity cannot be reassigned by reconfiguration"
        );
    }
    let guidance:Vec<_>=profile.guidance.iter().chain(&profile.architecture).map(|p| {let path=safe_path(&root,p)?;Ok(serde_json::json!({"path":p,"sha256":if path.is_file(){Some(digest(&fs::read(path)?))}else{None}}))}).collect::<Result<_>>()?;
    let mut templates = BTreeMap::new();
    for (name, body) in crate::adapters::templates() {
        templates.insert(name, digest(body.as_bytes()));
    }
    let mut desired = BTreeMap::from([
        (format!("{CONFIG}/profile.json"), json(profile)?),
        (
            format!("{CONFIG}/lock.json"),
            json(
                &serde_json::json!({"schema_version":1,"core":VERSION,"adapter_protocol":1,"prompts":templates,"rubric":"1","skills":"none","profile_sha256":digest(json(profile)?.as_bytes())}),
            )?,
        ),
        (format!("{CONFIG}/context.json"), json(&guidance)?),
        (
            format!("{CONFIG}/scenarios.json"),
            json(&pilot_scenarios(&profile.template))?,
        ),
        (
            format!("{CONFIG}/.gitignore"),
            "/runs/\n/transactions/\n/discovery/\n/workflows/\n/retrospectives/\n/harness/\n/kanban/\n/console-probe.json\n/review-links/\n/.lock\n/.workflow-lock\n".into(),
        ),
    ]);
    let mut ownership = Ownership::default();
    let mut changes = vec![];
    for (path, after) in &desired {
        let before = text_at(&root, path)?;
        if let Some(ref content) = before {
            ensure!(
                old.files.get(path) == Some(&digest(content.as_bytes())),
                "Manual or unowned file conflict: {path}. Preserve it and resolve explicitly before reconfiguring."
            );
        }
        ownership
            .files
            .insert(path.clone(), digest(after.as_bytes()));
        if before.as_ref() != Some(after) {
            changes.push(Change {
                path: path.clone(),
                before,
                after: Some(after.clone()),
            });
        }
    }
    desired.clear();
    let before = text_at(&root, &own_path)?;
    let after = json(&ownership)?;
    if before.as_ref() != Some(&after) {
        changes.push(Change {
            path: own_path,
            before,
            after: Some(after),
        });
    }
    Ok(SetupPlan {
        id: Uuid::new_v4().to_string(),
        root,
        action: "configure".into(),
        changes,
    })
}
pub fn apply(plan: &SetupPlan) -> Result<()> {
    let _guard = ProjectLock::acquire(&plan.root)?;
    ensure!(
        pending(&plan.root)?.is_empty(),
        "Recover interrupted setup first"
    );
    validate_targets(plan)?;
    if let Some(text) = text_at(&plan.root, &format!("{CONFIG}/ownership.json"))? {
        let ownership: Ownership = serde_json::from_str(&text)?;
        for (path, hash) in ownership.files {
            ensure!(
                text_at(&plan.root, &path)?.is_some_and(|s| digest(s.as_bytes()) == hash),
                "Installer-owned file changed after preview: {path}"
            );
        }
    }
    for c in &plan.changes {
        ensure!(
            text_at(&plan.root, &c.path)? == c.before,
            "File changed after preview: {}",
            c.path
        );
    }
    if plan.changes.is_empty() {
        return Ok(());
    }
    let path = safe_path(
        &plan.root,
        &format!("{CONFIG}/transactions/{}.json", plan.id),
    )?;
    let mut journal = Journal {
        plan: plan.clone(),
        state: "pending".into(),
    };
    atomic(&path, json(&journal)?.as_bytes())?;
    for c in &plan.changes {
        write_change(&plan.root, c, false)?;
    }
    journal.state = "complete".into();
    atomic(&path, json(&journal)?.as_bytes())?;
    Ok(())
}
fn validate_targets(plan: &SetupPlan) -> Result<()> {
    for c in &plan.changes {
        ensure!(
            [
                "profile.json",
                "lock.json",
                "context.json",
                "scenarios.json",
                ".gitignore",
                "ownership.json"
            ]
            .iter()
            .any(|name| c.path == format!("{CONFIG}/{name}")),
            "Unowned setup target"
        );
        safe_path(&plan.root, &c.path)?;
    }
    Ok(())
}
fn write_change(root: &Path, c: &Change, rollback: bool) -> Result<()> {
    let expected = if rollback { &c.after } else { &c.before };
    let target = if rollback { &c.before } else { &c.after };
    let current = text_at(root, &c.path)?;
    if &current == target {
        return Ok(());
    }
    ensure!(
        &current == expected,
        "Recovery conflict: {} was subsequently edited",
        c.path
    );
    let path = safe_path(root, &c.path)?;
    match target {
        Some(s) => atomic(&path, s.as_bytes())?,
        None => {
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
    }
    Ok(())
}
pub fn recover(root: &Path, rollback: bool) -> Result<usize> {
    let _guard = ProjectLock::acquire(root)?;
    let paths = pending(root)?;
    for p in &paths {
        let mut j: Journal = serde_json::from_str(&fs::read_to_string(p)?)?;
        ensure!(
            fs::canonicalize(root)? == j.plan.root,
            "Transaction belongs to another project root"
        );
        validate_targets(&j.plan)?;
        for c in &j.plan.changes {
            let current = text_at(root, &c.path)?;
            ensure!(
                current == c.before || current == c.after,
                "Recovery conflict: {}",
                c.path
            );
        }
        for c in &j.plan.changes {
            write_change(root, c, rollback)?;
        }
        j.state = if rollback { "rolled_back" } else { "complete" }.into();
        atomic(p, json(&j)?.as_bytes())?;
    }
    Ok(paths.len())
}
pub fn detach_preview(root: &Path) -> Result<SetupPlan> {
    let root = fs::canonicalize(root)?;
    ensure!(
        pending(&root)?.is_empty(),
        "Recover interrupted setup first"
    );
    let own_path = format!("{CONFIG}/ownership.json");
    let own_text = text_at(&root, &own_path)?.context("No installation ownership record")?;
    let own: Ownership = serde_json::from_str(&own_text)?;
    let mut changes = vec![];
    for (path, hash) in own.files {
        let before = text_at(&root, &path)?;
        ensure!(
            before
                .as_ref()
                .is_some_and(|s| digest(s.as_bytes()) == hash),
            "Modified or missing owned file: {path}; detach will preserve it until resolved"
        );
        changes.push(Change {
            path,
            before,
            after: None,
        });
    }
    changes.push(Change {
        path: own_path,
        before: Some(own_text),
        after: None,
    });
    Ok(SetupPlan {
        id: Uuid::new_v4().to_string(),
        root,
        action: "detach".into(),
        changes,
    })
}
pub fn rollback_preview(root: &Path, id: &str) -> Result<SetupPlan> {
    ensure!(Uuid::parse_str(id).is_ok(), "Invalid transaction ID");
    let root = fs::canonicalize(root)?;
    let path = safe_path(&root, &format!("{CONFIG}/transactions/{id}.json"))?;
    let j: Journal = serde_json::from_str(&fs::read_to_string(path)?)?;
    ensure!(
        j.plan.root == root && j.state == "complete",
        "Transaction is not a completed operation for this root"
    );
    let plan = SetupPlan {
        id: Uuid::new_v4().to_string(),
        root,
        action: "rollback".into(),
        changes: j
            .plan
            .changes
            .into_iter()
            .map(|c| Change {
                path: c.path,
                before: c.after,
                after: c.before,
            })
            .collect(),
    };
    validate_targets(&plan)?;
    Ok(plan)
}
pub fn pilot_scenarios(template: &str) -> Vec<serde_json::Value> {
    let cases = if template == "meal-map" {
        vec![
            "Useful scoped meal proposal",
            "Preserve unrelated meals",
            "Respect dietary constraints",
            "Complete save journey",
            "Clear loading state",
            "Failure and retry",
            "Cancel draft safely",
            "Persist after relaunch",
            "Empty grocery scope",
            "Accessible recovery",
        ]
    } else {
        vec![
            "Useful scoped proposal",
            "Preserve existing data",
            "Respect constraints",
            "Complete primary task",
            "Explain pending state",
            "Failure and retry",
            "Cancel safely",
            "Persist after restart",
            "Handle empty input",
            "Accessible recovery",
        ]
    };
    cases.iter().enumerate().map(|(i,title)|serde_json::json!({"id":format!("P{:02}",i+1),"title":title,"split":if i>=8{"held_out"}else{"development"},"category":"product_quality","rubric_version":"1","anchors":{"weak":"Task fails or evidence is missing","acceptable":"Observable approved outcome achieved with recovery","strong":"Outcome and recovery are clear, useful and supported by user evidence"},"status":"not_run"})).collect()
}
#[derive(Debug, Serialize)]
pub struct Readiness {
    pub ready: bool,
    pub blockers: Vec<String>,
    pub missing_optional: Vec<String>,
    pub suggestions: Vec<String>,
}
pub fn readiness(root: &Path, p: &Profile) -> Readiness {
    let mut blockers = vec![];
    let mut optional = vec![];
    if p.product_brief.trim().is_empty() {
        blockers.push("Supply the product brief".into());
    }
    if p.runtime.command.program.is_empty() {
        blockers.push("Configure the runtime command".into());
    } else if !executable_available(&p.runtime.command.program) {
        blockers.push("Runtime executable is unavailable".into());
    }
    if p.runtime.planner_model.is_empty() {
        blockers.push("Select a high-reasoning planner model".into());
    }
    if p.review.command.is_none() {
        blockers.push(format!(
            "Configure the {} review bridge with attributable human decisions",
            p.review.kind
        ));
    }
    if p.review.reviewers.is_empty() {
        blockers.push("Configure authorised human reviewer IDs".into());
    }
    if let Some(cmd) = &p.review.command
        && !executable_available(&cmd.program)
    {
        blockers.push("Review bridge executable is unavailable".into());
    }
    if let Some(env) = &p.review.credential_env
        && std::env::var_os(env).is_none()
    {
        blockers.push(format!(
            "Credential environment variable {env} is unavailable"
        ));
    }
    if p.review.kind == "notion" {
        for name in ["NOTION_TOKEN", "NOTION_PAGE_ID"] {
            if std::env::var_os(name).is_none() {
                blockers.push(format!("Set {name} in the runner environment"));
            }
        }
    }
    if p.checks.is_empty() {
        blockers.push("Define the project's required verification checks".into());
    }
    for c in &p.checks {
        if !executable_available(&c.command.program) {
            let msg = format!("Check {} executable unavailable", c.id);
            if c.required {
                blockers.push(msg)
            } else {
                optional.push(msg)
            }
        }
    }
    for path in &p.architecture {
        if !root.join(path).is_file() {
            blockers.push(format!("Missing architecture reference {path}"));
        }
    }
    let suggestions=vec![if root.join("AGENTS.md").exists(){"Existing AGENTS.md is preserved and will be snapshotted".into()}else{"No root AGENTS.md detected; add reviewed repository guidance when available".into()},"Runtime high-reasoning and review provenance probes require an explicit doctor --probe action before dispatch".into()];
    Readiness {
        ready: blockers.is_empty(),
        blockers,
        missing_optional: optional,
        suggestions,
    }
}
pub fn executable_available(program: &str) -> bool {
    let candidate = |p: PathBuf| {
        if !p.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            true
        }
    };
    if program.contains(std::path::MAIN_SEPARATOR) {
        return candidate(PathBuf::from(program));
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|p| candidate(p.join(program))))
}
