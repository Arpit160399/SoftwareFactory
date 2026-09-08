# Release acceptance and pilot evidence

Specification: `multi-agent-product-workflow-plan.md`, version 0.6. Contract and role-template version: 0.1.0.

This matrix defines acceptance obligations. An entry is not a claim that its implementation or validation has passed. Record actual test results and external evidence separately. Source-level review, synthetic fixtures, a live adapter check and a real product pilot establish different things.

The initial package is a local Rust application with a setup TUI, shared CLI services, a guarded workflow engine and a configurable command bridge. Test fixtures can prove these package contracts. They cannot supply real Meal Map approvals, model capability, Notion decision provenance, iOS journey evidence or product outcomes.

## Package acceptance matrix

| ID | Specification | Required observable result | Evidence required |
| --- | --- | --- | --- |
| SET-01 | §2 setup | Exact project root, selected profile/release, proposed files, permissions and missing tools appear before setup applies. Detection executes no project scripts or paid/external operations. | Shared-service tests and TUI inspection. |
| SET-02 | §2 setup | Confirmation applies only the previewed configuration; a file changed since preview produces a conflict. Existing AGENTS.md and app files survive. | Before/after filesystem assertions. |
| SET-03 | §2 lifecycle | Repeat setup creates no duplicate records and does not overwrite manually edited generated files. | First/repeat/conflicting setup tests. |
| SET-04 | §2 lifecycle | Interrupted setup has a recoverable journal and cannot be mistaken for valid complete setup; rollback preserves unrelated and subsequently edited files. | Failure injection at transaction boundaries and recovery tests. |
| SET-05 | §2 lifecycle | A project upgrade previews changes, retains previous configuration/release and requires exact-version adoption authority. Another project's pin and active-run snapshots are unchanged. | Two-project upgrade and rollback tests. |
| SET-06 | §2 lifecycle | Detach previews targets and removes only confirmed unchanged installer-owned content, retaining app code, existing guidance, credentials, evidence and recovery information. | Detach tests including manually edited owned files. |
| SET-07 | §2 distribution | Launcher and versioned machine releases remain separate from project configuration/history. Setup ends without feature dispatch or implicit build/merge/release approval. | Installation-layout and no-dispatch tests. |
| CFG-01 | §2 profile | Stable project ID, context/guidance references, reviewer/adapter selection, named required/optional checks, budgets, fixtures and artifact location validate through a shared service. Secrets are referenced by environment-variable name, never embedded. | Valid and invalid profile tests. |
| CFG-02 | §2 capabilities | Required unavailable capabilities block the dependent stage with a reason; optional missing capabilities remain visible as coverage gaps. Planner support includes effective high reasoning. | Capability success/unavailable/failure tests. |
| CORE-01 | §3–6 | No technical-planning or writing dispatch precedes a valid build decision for the exact active project, proposal revision and action. Feedback, silence, arbitrary status and imported approved_by fields never grant authority. | Negative decision and transition tests. |
| CORE-02 | §6 | Human acceptance, merge authority, release authority and harness adoption are distinct. Superseding an approved proposal does not transfer permission. | Action/revision isolation tests. |
| CORE-03 | §4 | Planner, implementer and reviewer use distinct contexts; only the implementer receives write authority. A reviewer inspects the frozen candidate and pre-run rules. | Runtime request inspection and role-identity rejection tests. |
| CORE-04 | §4 | Failed review returns unmet criterion IDs and evidence to another planning iteration. There is no fixed repair count or limit-equals-success path. | Failing-first, repairing-second synthetic run. |
| CORE-05 | §4, §8–9 | Readiness requires every mandatory criterion and required check to pass with inspectable evidence on the exact reviewed revision; guidance must be consistent. Missing/failed/not-run results cannot be averaged away. | Gate tests for each incomplete result and full successful fixture. |
| CORE-06 | §4 | Budget exhaustion, no viable next experiment, missing required capability or a needed decision creates a blocked/incomplete handoff with reason, unresolved IDs and resume point. | Budget/block/resume tests. |
| CORE-07 | §2, §11 | Runs snapshot core/profile/adapter/prompt/skill/rubric versions and retrievable content, approved spec, source baseline and effective instruction paths/revisions. Resume uses these rather than mutable current files. | Snapshot integrity and configuration-change tests. |
| CORE-08 | §4, §7 | Dispatch intent is durable before an external call. Recovery reconciles the same idempotency key; an ambiguous submission blocks instead of blindly duplicating the job. | Crash-before/after-submission fixtures. |
| CORE-09 | §2, §4 | One active feature per project, explicit writer ownership and shared-resource serialization prevent conflicting writes. Records, decisions, context and artifacts stay project/run-scoped. | Concurrent access and cross-project isolation tests. |
| CORE-10 | §4 | A changed candidate invalidates readiness and affected evidence. Reused evidence requires an explicit applicability record. | Candidate-change and stale-result rejection tests. |
| ADP-01 | §2, §4 | The command runtime validates its protocol/capabilities, dispatches bounded role requests, reports progress/cancellation/results and supports idempotent reconciliation. Errors cannot masquerade as completion. | Substitute command-bridge conformance fixture. |
| ADP-02 | §6–7 | Review packet retries update the same record. The adapter verifies attributable human decision provenance; unverified inputs never authorize dispatch. Summary sync failure is separate from previously granted authority. | Review protocol tests plus a separately identified live provenance check. |
| ADP-03 | §8–9 | Named authorized checks retain execution state separately from verdict and record candidate, environment, fixtures, evidence and errors. Detection alone never launches a check. | Passing/failing/unavailable/timeout check fixtures. |
| GUIDE-01 | §2 guidance | Relevant root/scoped instructions and overrides are resolved and recorded without silent omission/truncation. Fresh handoffs receive the correct context while pre-run policy remains available. | Nested guidance/override/size-limit fixtures. |
| GUIDE-02 | §2, §4 | Planner reports guidance impact; implementer records narrow factual changes or no-change; reviewer checks evidence and preserves governing authority. | Handoff records plus an independent review fixture. |
| PROD-01 | §5–6, §8 | Discovery findings retain sources/uncertainty; at most three opportunities feed a revisioned proposal with stable criteria, journeys, scope and evidence gaps. Direct product-planning entry is supported. | Structured artifact validation and role-template inspection. |
| REC-01 | §10–11 | Optional feedback retains exact target/revision and original wording separately from interpretation. Decisions and iterations remain append-only or explicitly superseded; secrets and unnecessary personal data are excluded. | Record round-trip, feedback non-authority and redaction tests. |
| LEARN-01 | §12 | An isolated candidate retains baseline, versioned rubric, held-out behavioral comparison, regressions and missing metrics; only an exact-version human decision permits adoption. | Comparison/adoption positive and negative tests. |
| PORT-01 | §13 | A synthetic non-iOS second profile runs through the same installed setup/core with different criteria/checks and a substitute adapter. Lifecycle/recovery/isolation tests require no core edits. | Two-project end-to-end test. |

A mocked command bridge must identify itself as a fixture in evidence. Its simulated planner or human decision does not establish a live capability. Template text is guidance; critical authorization, readiness and recovery requirements need mechanical enforcement.

## External Meal Map pilot milestones

These items require real project/environment evidence and are **not established by package fixture tests**. Keep them blocked or unverified until their evidence exists. Do not manufacture unavailable baseline metrics or claim the pilot is complete because the harness compiles.

| ID | Dependency or milestone | Required evidence |
| --- | --- | --- |
| PILOT-01 | Confirm Meal Map baseline and workspace ownership. | Agreed source revision and relevant uncommitted-work treatment; actual applicable guidance and architecture; permitted paths; present build/test health and evidence gaps. |
| PILOT-02 | Validate a real agent command bridge and high-reasoning planner. | Available model, supported and effective reasoning setting, distinct role contexts, permissions, bounded execution, cancellation and restart behavior. Configuration strings alone are insufficient. |
| PILOT-03 | Configure and verify the Notion review integration. | Authorized target records/property mapping, stable identifiers, replay-safe synchronization and trustworthy actor/source provenance for an exact-revision decision. A writable status field alone is insufficient. |
| PILOT-04 | Choose and approve one small observable feature. | Immutable proposal, stable criteria and journeys, approved scope/budgets and attributable human build decision. Installing this package supplies none of these. |
| PILOT-05 | Validate Promptfoo or an explicitly selected compatible evaluation adapter. | Isolated capability/cost check, authorized execution, versioned scenarios/rubrics, distinguishable evaluator/provider/product failures and saved output. |
| PILOT-06 | Supply real iOS journey and supporting test evidence. | Controlled storage/data/network/time fixtures; actual app revision, device/OS, visible/persisted outcomes, failed steps, result bundles and appropriate screenshots/accessibility/visual review. No Android scope is included. |
| PILOT-07 | Complete the first development loop. | Real planning, implementation, required checks and independent exact-candidate review; unresolved requirements repaired or explicitly blocked. Human exceptions remain exceptions. |
| PILOT-08 | Obtain final human review and demonstrate recoverability. | Attributable exact-candidate acceptance, separately recorded merge authority if requested, and recoverable instructions/iterations/context/decisions. Acceptance does not authorize release. |
| PILOT-09 | Exercise an actual learning comparison. | Reproducible case, isolated version, baseline/candidate and held-out evidence, regressions/cost/gaps and human adoption/defer/reject decision. Shared rollout stays opt-in. |
| PILOT-10 | Assess effectiveness and decide further trials. | Initial versus observed product acceptance, required coverage, review time, clarification, rework, elapsed time and actual cost where available. The next two features and real second-product adoption follow review, not automatic expansion. |

Trusted public release distribution also needs an agreed distribution and integrity/signing process. A locally built binary alone does not establish a trusted downloadable release.

## Evidence reporting

For each validation run, record date, package/source revision, command or test identity, result, artifact reference and material limitations. Use `passed`, `failed`, `blocked` or `not-run`; record execution as `pending`, `completed`, `unavailable` or `failed` separately where applicable. Never change these labels merely because a human accepts a documented exception.

Future workflow graph editing, user-authored flows, generalized model selection, recurring jobs and Android implementation/evaluation are outside this package's initial pilot contract. They are not substitutes for any requirement above.
