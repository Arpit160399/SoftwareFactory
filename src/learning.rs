//! Project-local, evidence-backed harness experiments. Adoption records authority;
//! applying a release or changing setup is a separate explicit operation.
use crate::{
    engine::{DecisionAction, DecisionClaim, DecisionVerifier, VerifiedDecision},
    setup::{self, CONFIG, ProjectLock},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionArtifact {
    pub version: String,
    pub artifact_ref: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub scenario_id: String,
    pub split: String,
    pub rubric_version: String,
    pub baseline_evidence_ref: String,
    pub candidate_evidence_ref: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateInput {
    pub owner: String,
    pub disposition: String,
    pub diagnosis: String,
    pub reproducible_case: String,
    pub diagnostic_evidence_refs: Vec<String>,
    pub baseline: VersionArtifact,
    pub candidate: Option<VersionArtifact>,
    #[serde(default)]
    pub comparisons: Vec<Comparison>,
    #[serde(default)]
    pub gains: Vec<String>,
    #[serde(default)]
    pub regressions: Vec<String>,
    #[serde(default)]
    pub gaps: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrialEvidence {
    project_id: String,
    run_id: String,
    version_hash: String,
    scenario_id: String,
    split: String,
    rubric_version: String,
    exercise: String,
    execution: String,
    outcome: String,
    context_ids: Vec<String>,
    trace_ref: String,
    observations: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FrozenArtifact {
    source_ref: String,
    snapshot_ref: String,
    sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessCandidate {
    pub schema_version: u32,
    pub id: String,
    pub project_id: String,
    pub run_id: String,
    pub revision: String,
    pub created_at: u64,
    pub input: CandidateInput,
    pub baseline_hash: String,
    pub candidate_hash: Option<String>,
    pub comparison_summary: Value,
    artifacts: Vec<FrozenArtifact>,
    pub disposition: String,
    pub decisions: Vec<DecisionClaim>,
}
fn nonempty(v: &str, label: &str) -> Result<()> {
    ensure!(!v.trim().is_empty(), "{label} is required");
    Ok(())
}
fn record_path(root: &Path, id: &str) -> Result<PathBuf> {
    ensure!(Uuid::parse_str(id).is_ok(), "Invalid harness candidate ID");
    setup::safe_path(root, &format!("{CONFIG}/harness/{id}/candidate.json"))
}
fn source_bytes(root: &Path, reference: &str) -> Result<Vec<u8>> {
    let path = setup::safe_path(root, reference)?;
    ensure!(path.is_file(), "Evidence file is missing: {reference}");
    ensure!(
        fs::metadata(&path)?.len() <= 16 * 1024 * 1024,
        "Evidence exceeds 16 MiB: {reference}"
    );
    let bytes = fs::read(path)?;
    ensure!(!bytes.is_empty(), "Evidence file is empty: {reference}");
    Ok(bytes)
}
fn collect(root: &Path, reference: &str, files: &mut BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    if let Some(bytes) = files.get(reference) {
        return Ok(bytes.clone());
    }
    let bytes = source_bytes(root, reference)?;
    files.insert(reference.into(), bytes.clone());
    Ok(bytes)
}
fn fingerprint(record: &HarnessCandidate) -> Result<String> {
    Ok(setup::digest(&serde_json::to_vec(
        &json!({"project_id":record.project_id,"run_id":record.run_id,"input":record.input,"baseline_hash":record.baseline_hash,"candidate_hash":record.candidate_hash,"comparison_summary":record.comparison_summary,"artifacts":record.artifacts}),
    )?))
}
fn load(root: &Path, id: &str) -> Result<HarnessCandidate> {
    let record: HarnessCandidate = serde_json::from_slice(&fs::read(record_path(root, id)?)?)?;
    ensure!(
        record.schema_version == 1 && record.id == id,
        "Harness record identity/schema mismatch"
    );
    ensure!(
        record.project_id == setup::load_profile(root)?.project_id,
        "Harness candidate belongs to another project"
    );
    ensure!(
        record.revision == fingerprint(&record)?,
        "Harness candidate changed after evaluation"
    );
    for artifact in &record.artifacts {
        ensure!(
            setup::digest(&source_bytes(root, &artifact.snapshot_ref)?) == artifact.sha256,
            "Frozen evidence changed: {}",
            artifact.snapshot_ref
        );
    }
    Ok(record)
}
fn validate_trial(
    root: &Path,
    reference: &str,
    project: &str,
    run_id: &str,
    version_hash: &str,
    comparison: &Comparison,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<TrialEvidence> {
    let evidence: TrialEvidence = serde_json::from_slice(&collect(root, reference, files)?)
        .context("Comparison evidence must be a structured workflow harness trial")?;
    ensure!(
        evidence.project_id == project && evidence.run_id == run_id,
        "Trial evidence belongs to another project or run"
    );
    ensure!(
        evidence.version_hash == version_hash,
        "Trial was evaluated against a different harness artifact"
    );
    ensure!(
        evidence.scenario_id == comparison.scenario_id
            && evidence.split == comparison.split
            && evidence.rubric_version == comparison.rubric_version,
        "Comparison must use the same scenario, split and fixed rubric"
    );
    ensure!(
        evidence.exercise == "workflow_harness",
        "Standalone model scores do not establish harness behavior"
    );
    ensure!(
        evidence.execution == "completed"
            && matches!(evidence.outcome.as_str(), "passed" | "failed"),
        "Comparison execution is unavailable or incomplete"
    );
    ensure!(
        !evidence.observations.is_empty()
            && evidence.observations.iter().all(|v| !v.trim().is_empty()),
        "Comparison needs behavioral observations"
    );
    ensure!(
        !evidence.context_ids.is_empty()
            && evidence.context_ids.iter().all(|v| !v.trim().is_empty()),
        "Comparison needs fresh-context execution identity"
    );
    ensure!(
        evidence.context_ids.iter().collect::<HashSet<_>>().len() == evidence.context_ids.len(),
        "Trial repeats a context identity"
    );
    collect(root, &evidence.trace_ref, files)?;
    Ok(evidence)
}

pub fn create(root: &Path, run_id: &str, input: Value) -> Result<Value> {
    let _guard = ProjectLock::acquire(root)?;
    let run = crate::adapters::load_run(root, run_id)?;
    let input: CandidateInput = serde_json::from_value(input)?;
    ensure!(
        matches!(
            input.owner.as_str(),
            "core" | "profile" | "adapter" | "prompt" | "skill" | "none"
        ),
        "Select one harness improvement owner"
    );
    ensure!(
        matches!(
            input.disposition.as_str(),
            "candidate" | "no_change" | "deferred"
        ),
        "Unsupported experiment disposition"
    );
    nonempty(&input.diagnosis, "Diagnosis")?;
    nonempty(&input.reproducible_case, "Reproducible case")?;
    nonempty(&input.baseline.version, "Baseline version")?;
    ensure!(
        !input.diagnostic_evidence_refs.is_empty(),
        "Diagnosis needs actual source evidence"
    );
    let mut files = BTreeMap::new();
    for reference in &input.diagnostic_evidence_refs {
        collect(root, reference, &mut files)?;
    }
    let baseline_hash = setup::digest(&collect(root, &input.baseline.artifact_ref, &mut files)?);
    let candidate_hash = if let Some(candidate) = &input.candidate {
        nonempty(&candidate.version, "Candidate version")?;
        Some(setup::digest(&collect(
            root,
            &candidate.artifact_ref,
            &mut files,
        )?))
    } else {
        None
    };
    if input.disposition == "candidate" {
        ensure!(
            input.owner != "none",
            "An adoption candidate needs a specific owner"
        );
        ensure!(
            candidate_hash
                .as_ref()
                .is_some_and(|hash| hash != &baseline_hash),
            "Candidate must be an isolated, changed harness artifact"
        );
        ensure!(
            input
                .candidate
                .as_ref()
                .is_some_and(|c| c.version != input.baseline.version),
            "Candidate needs its own version identity"
        );
        ensure!(
            input.comparisons.iter().any(|c| c.split == "development")
                && input.comparisons.iter().any(|c| c.split == "held_out"),
            "Candidate requires development and held-out comparisons"
        );
    }
    let mut contexts = HashSet::new();
    let mut scenarios = HashSet::new();
    let mut improved = 0;
    let mut regressed = 0;
    let mut unchanged = 0;
    for comparison in &input.comparisons {
        nonempty(&comparison.scenario_id, "Scenario ID")?;
        nonempty(&comparison.rubric_version, "Rubric version")?;
        ensure!(
            matches!(comparison.split.as_str(), "development" | "held_out"),
            "Unknown scenario split"
        );
        ensure!(
            scenarios.insert(comparison.scenario_id.clone()),
            "A scenario cannot appear in both development and held-out sets"
        );
        let baseline = validate_trial(
            root,
            &comparison.baseline_evidence_ref,
            &run.project_id,
            run_id,
            &baseline_hash,
            comparison,
            &mut files,
        )?;
        let candidate = validate_trial(
            root,
            &comparison.candidate_evidence_ref,
            &run.project_id,
            run_id,
            candidate_hash
                .as_deref()
                .context("Comparison is missing a candidate artifact")?,
            comparison,
            &mut files,
        )?;
        for context in baseline.context_ids.iter().chain(&candidate.context_ids) {
            ensure!(
                contexts.insert(context.clone()),
                "Each trial must use fresh contexts; reused {context}"
            );
        }
        match (baseline.outcome.as_str(), candidate.outcome.as_str()) {
            ("failed", "passed") => improved += 1,
            ("passed", "failed") => regressed += 1,
            _ => unchanged += 1,
        }
    }
    let id = Uuid::new_v4().to_string();
    let mut artifacts = vec![];
    // Validate everything before writing the isolated immutable evidence package.
    for (index, (source_ref, bytes)) in files.into_iter().enumerate() {
        let snapshot_ref = format!("{CONFIG}/harness/{id}/evidence/{index}.artifact");
        setup::atomic(&setup::safe_path(root, &snapshot_ref)?, &bytes)?;
        artifacts.push(FrozenArtifact {
            source_ref,
            snapshot_ref,
            sha256: setup::digest(&bytes),
        });
    }
    let mut record = HarnessCandidate {
        schema_version: 1,
        id: id.clone(),
        project_id: run.project_id,
        run_id: run_id.into(),
        revision: String::new(),
        created_at: setup::timestamp(),
        disposition: input.disposition.clone(),
        input,
        baseline_hash,
        candidate_hash,
        comparison_summary: json!({"improved":improved,"regressed":regressed,"unchanged":unchanged,"claim_limit":"Recorded behavioral trials only; no statistical or product-adoption claim", "setup_changed":false}),
        artifacts,
        decisions: vec![],
    };
    record.revision = fingerprint(&record)?;
    setup::atomic(&record_path(root, &id)?, setup::json(&record)?.as_bytes())?;
    Ok(serde_json::to_value(record)?)
}

pub fn read(root: &Path, id: &str) -> Result<Value> {
    Ok(serde_json::to_value(load(root, id)?)?)
}
pub fn list(root: &Path) -> Result<Vec<Value>> {
    let dir = setup::safe_path(root, &format!("{CONFIG}/harness"))?;
    let mut result = vec![];
    if !dir.exists() {
        return Ok(result);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let id = entry.file_name().to_string_lossy().into_owned();
            let path = record_path(root, &id)?;
            if path.is_file() {
                result.push(read(root, &id)?);
            }
        }
    }
    result.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(result)
}
pub fn adopt(
    root: &Path,
    id: &str,
    claim: DecisionClaim,
    verifier: &impl DecisionVerifier,
) -> Result<Value> {
    let _guard = ProjectLock::acquire(root)?;
    let mut record = load(root, id)?;
    ensure!(
        claim.action == DecisionAction::HarnessAdopt,
        "Feature acceptance does not grant harness adoption"
    );
    ensure!(
        claim.project_id == record.project_id
            && claim.run_id == record.run_id
            && claim.artifact_revision == record.revision,
        "Adoption must match this project, source run and exact evaluated candidate"
    );
    VerifiedDecision::verify(claim.clone(), verifier)?;
    if let Some(existing) = record.decisions.iter().find(|d| d.id == claim.id) {
        ensure!(
            serde_json::to_value(existing)? == serde_json::to_value(&claim)?,
            "Decision identity collision"
        );
        return Ok(serde_json::to_value(record)?);
    }
    ensure!(
        record.disposition == "candidate",
        "Only an evaluated adoption candidate can be adopted"
    );
    record.decisions.push(claim);
    record.disposition = "adoption_authorized".into();
    setup::atomic(&record_path(root, id)?, setup::json(&record)?.as_bytes())?;
    Ok(serde_json::to_value(record)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Budget, Criterion, Proposal, Run};
    struct Human;
    impl DecisionVerifier for Human {
        fn verify(&self, claim: &DecisionClaim) -> Result<()> {
            ensure!(claim.source == "human:confirmed", "untrusted decision");
            Ok(())
        }
    }
    fn fixture() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut profile = setup::Profile::template("generic", "learning-project");
        profile.product_brief = "Fixture product".into();
        let preview = setup::preview(root, &profile).unwrap();
        setup::apply(&preview).unwrap();
        let run = Run::new(
            profile.project_id.clone(),
            Proposal {
                revision: "p1".into(),
                title: "Fixture".into(),
                scope: "Fixture".into(),
                criteria: vec![Criterion {
                    id: "C1".into(),
                    description: "Works".into(),
                    required: true,
                    check_ids: vec![],
                }],
                journeys: vec![],
                required_checks: vec![],
            },
            serde_json::to_value(&profile).unwrap(),
            json!({}),
            Budget::default(),
        )
        .unwrap();
        crate::adapters::save_run(root, &run).unwrap();
        (dir, run.id)
    }
    fn file(root: &Path, name: &str, bytes: &[u8]) {
        setup::atomic(&root.join(name), bytes).unwrap();
    }
    fn candidate_input(root: &Path, run: &str) -> Value {
        file(root, "experiments/baseline.md", b"baseline prompt");
        file(root, "experiments/candidate.md", b"new prompt");
        file(
            root,
            "experiments/diagnosis.log",
            b"Trace: reviewer omitted required recovery behavior",
        );
        let mut comparisons = vec![];
        for (index, split) in ["development", "held_out"].iter().enumerate() {
            let scenario = format!("S{index}");
            for (arm, outcome, artifact) in [
                ("baseline", "failed", b"baseline prompt".as_slice()),
                ("candidate", "passed", b"new prompt".as_slice()),
            ] {
                let trace = format!("experiments/{index}-{arm}.trace");
                file(
                    root,
                    &trace,
                    b"dispatch started; requirement checked; observed behavior recorded",
                );
                let evidence = json!({"project_id":"learning-project","run_id":run,"version_hash":setup::digest(artifact),"scenario_id":scenario,"split":split,"rubric_version":"r1","exercise":"workflow_harness","execution":"completed","outcome":outcome,"context_ids":[format!("context-{index}-{arm}")],"trace_ref":trace,"observations":[format!("Recovery behavior {outcome}")]});
                file(
                    root,
                    &format!("experiments/{index}-{arm}.json"),
                    setup::json(&evidence).unwrap().as_bytes(),
                );
            }
            comparisons.push(json!({"scenario_id":scenario,"split":split,"rubric_version":"r1","baseline_evidence_ref":format!("experiments/{index}-baseline.json"),"candidate_evidence_ref":format!("experiments/{index}-candidate.json")}));
        }
        json!({"owner":"prompt","disposition":"candidate","diagnosis":"Recovery omission","reproducible_case":"Run fixture with recovery requirement","diagnostic_evidence_refs":["experiments/diagnosis.log"],"baseline":{"version":"v1","artifact_ref":"experiments/baseline.md"},"candidate":{"version":"v2","artifact_ref":"experiments/candidate.md"},"comparisons":comparisons,"gains":["Recovery checked"],"regressions":[],"gaps":["Small pilot sample"]})
    }
    fn claim(record: &Value) -> DecisionClaim {
        DecisionClaim {
            id: Uuid::new_v4().to_string(),
            project_id: record["project_id"].as_str().unwrap().into(),
            run_id: record["run_id"].as_str().unwrap().into(),
            actor: "human".into(),
            decided_at: "2026-09-08T10:00:00Z".into(),
            artifact_revision: record["revision"].as_str().unwrap().into(),
            source: "human:confirmed".into(),
            action: DecisionAction::HarnessAdopt,
        }
    }
    #[test]
    fn evaluated_adoption_preserves_setup_and_pinned_runs() {
        let (dir, run) = fixture();
        let root = dir.path();
        let before = fs::read(root.join(format!("{CONFIG}/lock.json"))).unwrap();
        let record = create(root, &run, candidate_input(root, &run)).unwrap();
        assert_eq!(record["comparison_summary"]["improved"], 2);
        let id = record["id"].as_str().unwrap();
        let approved = adopt(root, id, claim(&record), &Human).unwrap();
        assert_eq!(approved["disposition"], "adoption_authorized");
        assert_eq!(
            fs::read(root.join(format!("{CONFIG}/lock.json"))).unwrap(),
            before
        );
        assert_eq!(
            crate::adapters::load_run(root, &run).unwrap().pins,
            json!({})
        );
        assert_eq!(list(root).unwrap().len(), 1);
    }
    #[test]
    fn rejects_missing_held_out_and_standalone_score_evidence() {
        let (dir, run) = fixture();
        let root = dir.path();
        let mut input = candidate_input(root, &run);
        input["comparisons"].as_array_mut().unwrap().pop();
        assert!(create(root, &run, input).is_err());
        let input = candidate_input(root, &run);
        let evidence_path = root.join("experiments/0-candidate.json");
        let mut evidence: Value =
            serde_json::from_slice(&fs::read(&evidence_path).unwrap()).unwrap();
        evidence["exercise"] = json!("standalone_model_score");
        setup::atomic(&evidence_path, setup::json(&evidence).unwrap().as_bytes()).unwrap();
        assert!(create(root, &run, input).is_err());
    }
    #[test]
    fn adoption_rejects_wrong_authority_and_tampered_evidence() {
        let (dir, run) = fixture();
        let root = dir.path();
        let record = create(root, &run, candidate_input(root, &run)).unwrap();
        let id = record["id"].as_str().unwrap();
        let mut wrong = claim(&record);
        wrong.action = DecisionAction::Accept;
        assert!(adopt(root, id, wrong, &Human).is_err());
        let mut wrong = claim(&record);
        wrong.project_id = "another-project".into();
        assert!(adopt(root, id, wrong, &Human).is_err());
        let mut wrong = claim(&record);
        wrong.source = "agent:approved".into();
        assert!(adopt(root, id, wrong, &Human).is_err());
        let evidence = record["artifacts"][0]["snapshot_ref"].as_str().unwrap();
        file(root, evidence, b"changed after evaluation");
        assert!(adopt(root, id, claim(&record), &Human).is_err());
    }
    #[test]
    fn no_change_is_valid_without_manufacturing_an_adoption_candidate() {
        let (dir, run) = fixture();
        let root = dir.path();
        let mut input = candidate_input(root, &run);
        input["owner"] = json!("none");
        input["disposition"] = json!("no_change");
        input["candidate"] = Value::Null;
        input["comparisons"] = json!([]);
        let record = create(root, &run, input).unwrap();
        assert_eq!(record["disposition"], "no_change");
        assert!(adopt(root, record["id"].as_str().unwrap(), claim(&record), &Human).is_err());
    }
}
