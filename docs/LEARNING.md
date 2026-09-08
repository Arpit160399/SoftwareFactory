# Harness comparisons

`learn RUN_ID FILE` imports an evidence-backed comparison from an isolated experiment. The experiment itself must already have run under the relevant workspace/tool/budget authority; importing an artifact does not execute a model evaluation.

Input fields:

```json
{
  "owner": "prompt",
  "disposition": "candidate",
  "diagnosis": "A reproducible handoff failure",
  "reproducible_case": "Steps and expected improvement",
  "diagnostic_evidence_refs": ["experiments/diagnosis.txt"],
  "baseline": {"version": "1", "artifact_ref": "experiments/baseline.md"},
  "candidate": {"version": "2", "artifact_ref": "experiments/candidate.md"},
  "comparisons": [
    {"scenario_id": "fixed-1", "split": "development", "rubric_version": "1", "baseline_evidence_ref": "experiments/base-fixed.json", "candidate_evidence_ref": "experiments/new-fixed.json"},
    {"scenario_id": "held-1", "split": "held_out", "rubric_version": "1", "baseline_evidence_ref": "experiments/base-held.json", "candidate_evidence_ref": "experiments/new-held.json"}
  ],
  "gains": [], "regressions": [], "gaps": ["Model cost unavailable"]
}
```

Evidence paths are repository-relative, must exist, and cannot traverse symlinks. Owners are core/profile/adapter/prompt/skill/none. `no_change` and `deferred` dispositions retain diagnostics without manufacturing an improvement. A candidate requires paired development and held-out evidence.

Each trial JSON records `project_id`, `run_id`, artifact `version_hash`, `scenario_id`, `split`, `rubric_version`, `exercise: "workflow_harness"`, `execution: "completed"`, `outcome: "passed"` or `"failed"`, distinct `context_ids`, a nonempty `trace_ref` file and nonempty `observations`. Baseline/candidate context IDs must be disjoint. These structural checks do not establish the truth of a fabricated trace; the independent comparison reviewer must inspect actual behavior.

The package snapshots artifacts, traces and evidence. Their hashes contribute to an immutable candidate revision. The human adoption claim uses that revision, the originating run ID, and `action: "harness_adopt"`. The review bridge verifies the claim. Adoption records authority and retains regressions/gaps; it never replaces project configuration or active-run pins. A separately previewed setup operation applies the reviewed change.
