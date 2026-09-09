# Implementation and validation record

Date: 8 September 2026. Package: Software Factory 0.2.0. Host: macOS arm64. Rust: 1.98.1 (isolated temporary toolchain); package minimum 1.88.0. Dependency versions are recorded in `Cargo.lock`.

The user's implementation request supersedes the source plan's earlier planning-only status. The original plan remains unchanged as the specification. The user selected a configurable command runtime; the TUI uses Rust, Ratatui and Crossterm. No particular AI provider is required.

## Delivered package

- A shared setup/profile service and real keyboard-driven terminal wizard; exact project/file previews, explicit apply, incomplete-capability reporting and sample review-packet explanation.
- Versioned local installation, release inventory, explicit update/activation and rollback, including the binary and optional Python standard-library Notion/native-check bridges. New releases have local integrity manifests; managed launchers switch atomically under an installation lock. Reusable prompts are embedded in the binary. Installation does not change shell/global agent settings.
- Ownership-aware configuration, repeat setup, journaled interruption recovery, reconfiguration, rollback and safe detach. Previous configuration is retained. Another project's pin is unaffected.
- Project-scoped immutable proposal/configuration/context snapshots, adapter file hashes, authoritative decision verification, separate role contexts, criterion/check evidence gates, failed-review replanning, budgets and blocked resume points.
- Durable role/check intents, exact-key lookup, source/evidence tamper checks, one-active-feature ownership, resource locks and cancellation with unknown-termination blocking.
- Optional three-pass discovery, original-wording feedback, and frozen harness comparisons/adoption records with development/held-out trace requirements.
- Generic and Meal Map templates, ten product scenarios, role prompts, protocol documentation and a runnable synthetic bridge.

## Automated evidence

| Check | Result | Scope |
| --- | --- | --- |
| Rust format check | Scoped pass | Version-management source and tests are formatted. A later full-workspace check found concurrent adapter edits requiring formatting; those edits are outside this change. |
| Clippy, all targets | Qualified pass | Strict warnings-denied check reports unrelated `collapsible_if` findings in adapter and workflow code. Allowing that lint only passes; no remaining lint findings. |
| Core and harness unit tests | 18 passed | Fourteen guarded-core tests and four harness-learning tests. |
| Published-release updater tests | 8 passed | Metadata-only checks, complete download/install/activation, checksum and network failures, stable-release filtering, origin and size validation, no automatic downgrade, stale activation, immutable releases, retry and rollback. These use deterministic release fixtures. |
| Version-management integration tests | 11 passed | Read-only previews, immutable builds, manifests/tampering, safe launcher replacement, missing releases, stale previews, installer locking, version listing without a project, project upgrade/isolation/rollback, schema/manual-edit conflicts, inferred installation prefix and conflicting CLI flags. |
| Workflow integration tests | 14 passed | Full failed-review repair loop; approvals/candidate/evidence identity; initial source drift; recovered role and native-check results; discovery; setup repeat/recovery/conflict/rollback/detach; symlink escape; literal command arguments; installed binary and alternate second-project check. |
| Notion bridge offline tests | 5 passed | Exact human source succeeds; bot, wrong revision, wrong project and fabricated actor fail. No live API used. |
| Actual terminal smoke test | Passed | Real pseudo-terminal key events exercise preview, cancel, apply, repeat and detach; no dispatch during setup; guidance preserved; terminal settings restored. |
| Release build | Passed with toolchain warning | Optimized local executable with locked dependencies. The temporary toolchain could not strip debug information because its `rust-objcopy` could not load `libLLVM.dylib`; the executable was produced and ran successfully. |
| Real release upgrade smoke test | Passed | Actual 0.1.0 installation upgraded to 0.2.0, explicit project adoption, other-project isolation, configuration rollback, launcher rollback and retry. Reproduce with `python3 tests/upgrade_smoke.py /path/to/old/binary target/release/softwarefactory`. |

Version-management validation ran while other workflow files were being edited concurrently. The test results describe the tested snapshot, not a certification of subsequent workspace edits. The existing `source-manifest.json` remains the original 0.1.0 source snapshot.

The terminal smoke test interprets terminal cursor updates rather than searching raw escape sequences. Its initial stream-reading harness was corrected to consume output while waiting for process exit; the resulting terminal flow passed.

## Published release delivery

The updater-enabled CLI was checked against the real public GitHub endpoint: no matching published stable release was available (HTTP 404), and the command returned an actionable error without creating the installation prefix. A successful internet download cannot be validated until release assets are published. The download/verification/activation path passed deterministic transport tests.

The new tag-triggered workflow prepares a complete draft release for three native targets. Its YAML and shell syntax were checked locally; GitHub-hosted builds and draft creation have not been run. Publication remains a maintainer action. Asset SHA-256 digests are required from the GitHub API, with HTTPS enforcing the configured repository trust source; publisher signing/notarization is not configured.

## Whole-workflow loop validation

The complete product workflow now repeats across saved cycles: discovery, PM proposal, exact build approval, feature repair loop, human acceptance, evidence-grounded retrospective and optional verified harness adoption/deferral. No-action findings wait for direction; elapsed time and agent-dispatch budgets remain aggregate across cycles. Setup/release activation is still an explicit operation.

Final shared-workspace verification for this change:

- **72 Rust tests passed**: 40 library tests, 7 whole-workflow integration tests, 14 existing feature/setup integration tests and 11 version integration tests.
- **9 offline Notion tests passed**, including polling with authoritative author/time metadata, stale revision exclusion, and verified adoption deferral. No live account calls.
- Strict **Clippy all targets with warnings denied passed** and **full Rust formatting passed**.
- The **actual pseudo-terminal setup smoke test passed**, including terminal restoration and preserved guidance.
- Whole-workflow integration tests exercised two completed cycles with repair in each, independent human gates, prior-learning context, adoption and deferral, no-action waits, replay-safe child creation, the real CLI runner and deadline interruption with later lookup.
- Independent review reproduced last-dispatch recovery at exhausted budgets without duplicate work. It also interrupted active discovery: unknown termination remained blocked and prevented a second workflow; confirmed cancellation allowed a new workflow.

These loop tests use explicitly marked synthetic projects and deterministic commands. They validate the coordinator and evidence/authority boundaries, not real Codex/Claude operation, provider quality, live Notion integration, or a Meal Map product pilot. Direct production CLI runtime bridges are not included.

## Rust control panel and Notion board increment (9 September 2026)

The TUI now has Home, Workflow, Tasks, Review, History and Settings screens. It reuses the existing engine and setup transactions, runs commands outside the render loop, refreshes saved state during long operations, preserves task identity and known data across refresh errors, and pauses safely on close. Setup now includes limits, arguments, allowed paths and the optional reviewed Notion board mapping. Review details include bounded run-scoped diffs, findings and comparison evidence. The original wizard remains accessible with `tui --setup`.

The board adapter validates an existing Notion data source, updates only mapped properties, verifies write results, preserves notes, and uses a durable per-card queue with sequences, rate-limit backoff and conservative uncertain-create recovery. Failed cards do not starve unrelated work. Code and adapter tests use synthetic/offline projects only.

Validation for this increment:

- Rust suite: 81 tests (40 library, 9 console/board, 7 whole-workflow, 11 version and 14 existing workflow tests).
- Offline Python bridge suites: 19 tests, including lost creation responses, duplicate keys, deleted cards, old events, schema/type mismatch, rate limits and approval provenance.
- Actual pseudo-terminal tests cover all six screens, setup return/cancellation, capability probing, starting to an approval wait without unauthorized implementation, evidence details, task search/clear, settings, 44-column layout and terminal restoration. The existing setup PTY test also passes.
- Strict Clippy, Rust formatting and diff whitespace checks pass. The premium static audit has zero findings; DESIGN.md lint has zero errors/warnings. These static checks supplement terminal execution and do not certify browser/screen-reader behavior.
- The release executable is built locally. No published release or existing installation was replaced by this increment.

Live Notion connection permissions, an authorized test-board write and production Codex/Claude runtime bridges remain unconfigured/unverified. No live board, external notification, approval or product feature was created during this work. Use one synchronization host per workflow; the adapter does not claim a distributed atomic upsert.

## Original 0.1.0 independent review

A separate reviewer reproduced the original defects before integration fixes and reran targeted temporary-project reproductions afterward. Final reviewed coordinator/setup scope has no remaining material finding:

- Read-only violations remain blocked across repeated recovery attempts.
- Completed checks are not repeated; an incomplete check uses lookup.
- A running role can receive cancellation while the project lock is held.
- Unknown remote check termination remains blocked and prevents another feature; confirmed cancellation permits one.
- Deleted/changed evidence and a missing evidence index prevent acceptance.
- Source changes outside the recorded baseline and changed directly configured bridges prevent dispatch.
- Repository-relative implementation evidence resolves correctly when the selected project differs from the launch directory.

The review did not certify an arbitrary external bridge's sandbox or a real product deployment.

## Explicit implementation limits and remaining pilot work

- **Real runtime and Meal Map pilot remain unverified.** No real agent command, model capability, human decision source, Notion workspace mapping, approved Meal Map feature or iOS test command was configured or executed. `PILOT-01` through `PILOT-10` in `ACCEPTANCE.md` remain external milestones.
- Notion uses immutable child packet pages with attributed human comments. Database-specific dashboards/property mappings and live API behavior need the configured workspace's integration check.
- The native check adapter wraps synchronous commands and retains their result/log. It does not create application test fixtures, implement Meal Map UI tests or prove usability from exit status. Promptfoo and `.xcresult` production require real configured commands and authority.
- Runtime bridges are trusted configured executables. They must enforce tool/path/read-only restrictions and protect coordinator artifacts. Direct executable and existing file-argument bytes are pinned; transitive dependencies and remote service/model behavior require adapter validation.
- A source snapshot hashes the current workspace, including uncommitted and untracked source; it does not create a Git worktree. Baseline changes stop execution. Large files above 64 MiB and remote artifact stores require an additional repository/artifact adapter.
- Active-run schema/root migration and automated upgrades to unknown future releases are unavailable. Explicit project updates and configuration rollback are supported for schema 1; pinned mismatched releases are rejected, and prior release binaries may coexist.
- Imported harness comparisons must contain actual experiment evidence. The package validates and freezes it but does not automatically run a model experiment or silently adopt a changed harness. Human adoption is recorded separately from applying configuration.
- No private run history is shared globally. No merge, release, publishing, schedule, workflow graph/editor or Android pilot was performed.

These boundaries distinguish an implemented and tested local package from a completed live product pilot. The local package is ready to configure a real project; a live pilot requires the separate project inputs and feature approval described in the plan.
