# Implementation and validation record

Date: 8 September 2026. Package: Software Factory 0.1.0. Host: macOS arm64. Rust: 1.98.1 (isolated temporary toolchain); package minimum 1.88.0. Dependency versions are recorded in `Cargo.lock`.

The user's implementation request supersedes the source plan's earlier planning-only status. The original plan remains unchanged as the specification. The user selected a configurable command runtime; the TUI uses Rust, Ratatui and Crossterm. No particular AI provider is required.

## Delivered package

- A shared setup/profile service and real keyboard-driven terminal wizard; exact project/file previews, explicit apply, incomplete-capability reporting and sample review-packet explanation.
- Versioned local installation including the binary and optional Python standard-library Notion/native-check bridges. Reusable prompts are embedded in the binary. Installation does not change shell/global agent settings.
- Ownership-aware configuration, repeat setup, journaled interruption recovery, reconfiguration, rollback and safe detach. Previous configuration is retained. Another project's pin is unaffected.
- Project-scoped immutable proposal/configuration/context snapshots, adapter file hashes, authoritative decision verification, separate role contexts, criterion/check evidence gates, failed-review replanning, budgets and blocked resume points.
- Durable role/check intents, exact-key lookup, source/evidence tamper checks, one-active-feature ownership, resource locks and cancellation with unknown-termination blocking.
- Optional three-pass discovery, original-wording feedback, and frozen harness comparisons/adoption records with development/held-out trace requirements.
- Generic and Meal Map templates, ten product scenarios, role prompts, protocol documentation and a runnable synthetic bridge.

## Automated evidence

| Check | Result | Scope |
| --- | --- | --- |
| Rust format check | Passed | Rust source formatting. |
| Clippy, all targets, warnings denied | Passed | Static lint of library, CLI and tests. |
| Rust unit tests | 18 passed | Fourteen guarded-core tests and four harness-learning tests. |
| Rust integration tests | 14 passed | Full failed-review repair loop; approvals/candidate/evidence identity; initial source drift; recovered role and native-check results; discovery; setup repeat/recovery/conflict/rollback/detach; symlink escape; literal command arguments; installed binary and alternate second-project check. |
| Notion bridge offline tests | 5 passed | Exact human source succeeds; bot, wrong revision, wrong project and fabricated actor fail. No live API used. |
| Actual terminal smoke test | Passed | Real pseudo-terminal key events exercise preview, cancel, apply, repeat and detach; no dispatch during setup; guidance preserved; terminal settings restored. |
| Release build | Passed | Optimized local executable with locked dependencies. |

The terminal smoke test interprets terminal cursor updates rather than searching raw escape sequences. Its initial stream-reading harness was corrected to consume output while waiting for process exit; the resulting terminal flow passed.

## Independent review

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
- Active-run schema/root migration and automated upgrades to unknown future releases are unavailable. Reconfiguration/rollback are supported for schema 1; pinned mismatched releases are rejected, and prior release binaries may coexist.
- Imported harness comparisons must contain actual experiment evidence. The package validates and freezes it but does not automatically run a model experiment or silently adopt a changed harness. Human adoption is recorded separately from applying configuration.
- No private run history is shared globally. No merge, release, publishing, schedule, workflow graph/editor or Android pilot was performed.

These boundaries distinguish an implemented and tested local package from a completed live product pilot. The local package is ready to configure a real project; a live pilot requires the separate project inputs and feature approval described in the plan.
