# Software Factory

A Rust terminal application for setting up and running project-scoped product-development workflows. The setup wizard and command line share the same configuration, transaction and workflow services.

The package implements the initial tool described in `multi-agent-product-workflow-plan.md`: setup, explicit approvals, separate planner/implementer/reviewer contexts, verification, recovery, feedback and harness-comparison records. Its runtime command, human reviewers, Notion destination, approved feature and checks still need configuration and live validation. No external records or application repositories were changed while building this package.

## Build and launch

Requires Rust 1.88 or newer, with Cargo. Optional bundled bridges require Python 3. The Rust application is independent of Codex, any particular model provider, and Python when using other bridges.

```sh
cargo build --release --locked
./target/release/softwarefactory --project /absolute/path/to/project tui
```

The default TUI now opens a seven-screen control panel with live workflow controls, a local task board, review evidence, prerequisites and Notion sync health. See [the TUI guide](docs/TUI.md). Press E for setup, or use `tui --setup`.

The wizard accepts the exact project directory, generic or Meal Map template, product context, runtime executable, high-reasoning planner model, review bridge, human reviewer IDs and a checks JSON file. Tab changes fields, Ctrl+U clears a field, Enter previews, Y applies, and Esc returns or cancels. Small terminals scroll the selected field; preview and result screens support arrow-key scrolling.

Setup does not dispatch agents, execute repository scripts, download dependencies or create external records. It preserves existing `AGENTS.md`. Missing integration details produce incomplete readiness rather than invented values.

Install a versioned local binary and bundled bridges under a prefix you choose:

```sh
./target/release/softwarefactory install --prefix "$HOME/.local/softwarefactory"
./target/release/softwarefactory install --prefix "$HOME/.local/softwarefactory" --apply
```

The first command previews. The second writes `releases/0.2.0/`, the bundled bridges, a SHA-256 manifest, and an initial `bin/softwarefactory` launcher. Add that `bin` directory to your PATH yourself. `install` preserves an existing launcher; `update --apply` explicitly selects a release. Version switching supports macOS and Linux.

## Manage versions and update

Once an updater-enabled release is installed, users can check and install future published releases directly from their terminal:

```sh
softwarefactory update --check
softwarefactory update --apply
softwarefactory --version
```

`update` without `--apply` only checks and previews. It uses the latest stable GitHub release from `Arpit160399/SoftwareFactory`, selects the native build, downloads its binary and bridges, verifies every file against the GitHub API's SHA-256 digest, and atomically switches the launcher. An installed binary detects its installation prefix automatically; `--prefix /absolute/path` overrides it. A standalone binary defaults to `$HOME/.local/softwarefactory`. Updates require `curl` and access to the public GitHub repository.

Supported downloadable builds are Apple Silicon macOS, Intel macOS and x86-64 GNU Linux (built on Ubuntu 22.04). Drafts and prereleases are excluded. Missing releases, unsupported platforms, download failures and checksum mismatches produce errors while preserving the active release. GitHub's HTTPS release metadata is the trust source; publisher signing/notarization is not configured. A normal latest-version check never downgrades the active version.

List installed releases, or explicitly select an installed or published version for rollback:

```sh
softwarefactory versions
softwarefactory update --to 0.2.0 --apply
```

An already installed version can be selected offline. Old releases remain installed. New releases include local integrity manifests; legacy 0.1.0 installations are listed as `legacy_without_manifest`. Versions are immutable: a different build cannot overwrite the same version. Installation is serialized, and an interrupted activation can be retried.

For developers installing their own new build, use the explicit local option:

```sh
cargo build --release --locked
./target/release/softwarefactory update --local --prefix "$HOME/.local/softwarefactory"
./target/release/softwarefactory update --local --prefix "$HOME/.local/softwarefactory" --apply
```

An older binary without the remote updater needs this one-time local upgrade (or installation of a downloaded updater-enabled binary). Future updates then use `softwarefactory update --apply`.

Update each project's configuration separately, using the selected release:

```sh
softwarefactory --project /path/to/project project-update
softwarefactory --project /path/to/project project-update --apply
```

The preview shows the old/new release, exact configuration changes and transaction ID. Applying reuses the saved profile and journals prior configuration for `rollback TRANSACTION --apply`. Manual edits, interrupted setup and unsupported schemas must be resolved first. Other projects and existing runs keep their version pins and snapshots; continue an older run with `releases/VERSION/softwarefactory`. Project rollback and launcher rollback are separate operations. Updating configuration grants no feature, merge or release approval.

### Publish a version for terminal updates

[The release workflow](.github/workflows/release.yml) builds and tests three native targets when a `v*` tag is pushed. The tag must exactly match the stable version in `Cargo.toml` and the built executable. It uploads the binaries and matching bridges to a **draft** GitHub release and verifies all five assets have GitHub SHA-256 digests. Review and publish that draft to make the update visible to users. Existing published releases are not overwritten.

Maintainer steps: bump `Cargo.toml` and `Cargo.lock`, update `CHANGELOG.md`, commit the release, push the matching tag, then review and publish the completed draft. Creating a Git tag alone does not make an update available. The workflow has been added locally; no release was published by this change.

## Configure a project

For advanced fields, edit a separate profile draft and apply it through setup. Avoid directly editing installer-owned files; their checksums intentionally detect manual changes.

```sh
softwarefactory template generic --id my-project > /tmp/my-profile.json
# Edit /tmp/my-profile.json, then preview and apply:
softwarefactory --project /path/to/project setup --profile /tmp/my-profile.json
softwarefactory --project /path/to/project setup --profile /tmp/my-profile.json --apply
softwarefactory --project /path/to/project doctor
softwarefactory --project /path/to/project doctor --probe
```

`doctor` only inspects configuration and executable availability. `--probe` explicitly runs the configured capability probes. The runtime must report its supported roles, effective high-reasoning model support, scope/read-only enforcement, cancellation and idempotent lookup. The review bridge must support attributable decisions. Agent model/cost availability is never inferred from a configuration label.

The `meal-map` template supplies product context, architecture references and proposed journeys. It deliberately leaves commands, models and reviewers unconfigured. Validate its suggestions against the actual iOS repository before use. Generic projects supply their own product evidence contract.

Commands are executable paths and argument arrays, without shell expansion. Use absolute paths for adapters. An executable's bytes and existing file arguments are hashed into every run. Changes to those trusted files stop the run. Indirect dependencies, remote services and model behavior remain the bridge operator's responsibility; keep adapters outside agent-writable source and enforce the protocol's protected paths.

Configuration lives in `.product-workflow/`:

- `profile.json`, `lock.json`, `context.json`, `scenarios.json`: versionable project configuration.
- `ownership.json`: installer-owned files and checksums.
- `transactions/`: ignored journals containing before/after contents for recovery.
- `runs/<run-id>/`: ignored manifests, role requests/results, candidate snapshots and evidence.
- `discovery/`, `harness/`: project-specific research and comparison records.

Secrets are referenced by environment-variable names. Never place token values in profile arguments, product context, or agent output. Command stderr is withheld from error summaries. The native-check bridge redacts known secret environment values from retained logs. Large artifacts remain local; automatic retention/deletion and remote artifact hosting are not implemented.

Notion Kanban progress tracking and its required setup are defined in [Notion Kanban prerequisites](docs/NOTION-KANBAN-PREREQUISITES.md). The bridge now supports task-card synchronization with a durable local queue as well as review pages and verified decisions. Live workspace permissions still need validation.

## Repeat the whole workflow

Use `workflow start "PRODUCT QUESTION"`, then `workflow run WORKFLOW_ID` to repeat discovery, proposal, approved development, human acceptance and retrospective across cycles. Each cycle requires fresh decisions. The loop saves progress, carries prior learning forward, and pauses at approvals, budgets or no actionable opportunity. Add `--review` to explicitly synchronize and poll the configured review source. See [the whole-workflow guide](docs/WHOLE-WORKFLOW.md) for stopping, resuming, cycle limits and command-runtime requirements.

## Repair GitHub issues in a loop

In the TUI, press **7** for Issues, **G** to scan, and **R** to run the queue. **Space** pauses, **Enter** shows fix/regression evidence, **X** stops an attempt, and **T** retries it.

From the CLI, use `issues scan --repo OWNER/REPO --label bug` to create local issue tasks, then `issues run --repo OWNER/REPO --label bug` to poll and coordinate their repair. Each task passes through product planning, technical planning, implementation, mandatory fix and regression checks, independent review and human acceptance. Failed verification returns to planning. Repeat scans do not duplicate tasks, and interrupted handoffs resume from saved state.

Add `--until-wait` to return at the next human gate, or `--review` to explicitly synchronize and poll the configured review source. Setup requires authenticated GitHub CLI plus the project's runtime, reviewers and executable checks. See [the GitHub issue loop guide](docs/GITHUB-ISSUE-LOOP.md) for configuration, evidence requirements, status, recovery and retries.

## Run an approved feature

Create a proposal JSON using [examples/proposal.json](examples/proposal.json). Each mandatory criterion and affected journey must map to configured checks. The union of profile-, criterion-, journey- and planner-required checks remains required.

```sh
softwarefactory --project /path/to/project propose /tmp/proposal.json
softwarefactory --project /path/to/project status RUN_ID
softwarefactory --project /path/to/project sync-review RUN_ID
softwarefactory --project /path/to/project decision RUN_ID /tmp/build-decision.json
softwarefactory --project /path/to/project run RUN_ID
```

`propose` freezes configuration, templates, guidance, source hashes and the exact proposal, then waits for build approval. `sync-review` is an explicit external write. A decision file is only a **claim**; the configured review bridge independently verifies the human actor, source, project/run, action, timestamp and artifact revision. Imported JSON and agent-authored status never grant authority by themselves.

`step RUN_ID` performs one stage; `run RUN_ID` continues until review, blocking or a budget limit. Each planner/implementer/reviewer dispatch gets a distinct context. The planner uses the selected high-reasoning model; only the implementer receives scoped writing permission. Review includes the exact candidate, checks, guidance and unmet criteria. Failed reviews return to planning. Repeated identical findings with an unchanged plan pause for a new experiment. Limits never imply success.

Human acceptance, merge permission, release permission and harness adoption are separate actions. The application records these grants; it does not merge, publish or release source code. A changed candidate, changed evidence, unverified decision or missing required check blocks readiness. There is one active feature per project. Cancel the prior run before replacing its proposal; approval never transfers to a new revision.

```sh
softwarefactory --project /path/to/project cancel RUN_ID --reason "Stop this attempt"
softwarefactory --project /path/to/project feedback RUN_ID --stage reviewing --revision CANDIDATE_HASH "The recovery wording is unclear"
```

Cancellation requests can reach a running local command despite its project lock. Unknown remote role/check termination stays blocked until the bridge confirms it. For an interrupted submission, the next `step` looks up the same idempotency key; it does not blindly dispatch again. A bridge unable to reconcile its job must remain blocked. Restore a source baseline changed outside the recorded handoff or cancel and propose revised scope. Do not edit manifests to bypass these checks.

## Runtime, reviews and checks

See [docs/ADAPTER-PROTOCOL.md](docs/ADAPTER-PROTOCOL.md) for the command contract. Bring a bridge for your chosen agent runtime. The bundled deterministic fixture demonstrates the contract but is not an AI provider or trustworthy human approval source.

For Notion, configure the review command as `python3` with an absolute path to `bridges/notion_review.py`. Set `NOTION_TOKEN` and `NOTION_PAGE_ID` in the launch environment, and configure authorised Notion human user IDs. Share the chosen parent page with the integration and enable content/user/comment read permissions plus page insertion for explicit synchronization. The bridge stores immutable packet pages beneath that parent. It verifies human comments against server-supplied author/time metadata, rather than treating a page status as approval. Live integration has not been verified in your workspace.

The human comment body is exactly:

```json
{"project_id":"my-project","run_id":"RUN_UUID","artifact_revision":"EXACT_REVISION","action":"build"}
```

Submit a decision claim with the comment's actual `id`, `created_time` as `decided_at`, `created_by.id` as `actor`, and `source: "notion://PACKET_PAGE_UUID/COMMENT_UUID"`. The comment must remain retrievable and unresolved; resolved/deleted/edited or unavailable decisions fail fresh provenance checks. Authorisation cannot be inferred from a display name. Packet retries use the same key; ambiguous duplicate pages require reconciliation.

Use `bridges/native_check.py -- program argument ...` to map a synchronous native command's exit status and log to check evidence. It supports `xcodebuild`, `promptfoo eval`, `cargo test`, or an existing project test script. Configure actual arguments and controlled fixtures yourself. A successful command proves only that check; product usefulness, accessibility and visible/persisted journey outcomes need their own checks. Use a stable `resource` name to serialize access to a shared simulator.

## Discovery and learning

```sh
softwarefactory --project /path/to/project discover "Where does changing a saved plan become difficult?"
softwarefactory --project /path/to/project discovery-step DISCOVERY_ID
```

Three separate read-only passes produce sourced findings, at most three opportunities and a product proposal or a reasoned no-action/defer/research disposition. Run `discovery-step` for each pass. Its outputs never dispatch implementation or grant approval. Reviewed proposals enter `propose` separately.

```sh
softwarefactory --project /path/to/project learn RUN_ID /tmp/comparison.json
softwarefactory --project /path/to/project learning
softwarefactory --project /path/to/project learning CANDIDATE_ID
softwarefactory --project /path/to/project adopt CANDIDATE_ID /tmp/adoption-decision.json
```

Harness comparisons require actual baseline/candidate artifacts, paired development and held-out workflow traces, identical scenarios/rubrics and fresh contexts. Standalone scores are rejected as harness trials. No-change and deferred diagnostic records are supported. `adopt` verifies exact-candidate human authority and records it; it does not mutate active run pins or apply a release. Apply any reviewed setup change explicitly. See [docs/LEARNING.md](docs/LEARNING.md).

## Reconfigure, recover, remove

Reconfigure or adopt this release using a reviewed profile draft and the same `setup` preview/apply flow. Every changed configuration retains its prior bytes in a transaction journal.

```sh
softwarefactory --project /path/to/project setup-recover
softwarefactory --project /path/to/project setup-recover --rollback
softwarefactory --project /path/to/project rollback TRANSACTION_UUID
softwarefactory --project /path/to/project rollback TRANSACTION_UUID --apply
softwarefactory --project /path/to/project detach
softwarefactory --project /path/to/project detach --apply
```

Rollback and removal reject subsequent manual edits. Detach preserves app code, existing guidance, run evidence, discovery, harness records and shared releases. A moved project can be reconfigured; an active run with a different recorded root requires explicit migration rather than a guessed replacement. Automated active-run/schema migration is not provided.

## Validation and limits

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
python3 tests/notion_bridge_test.py
python3 tests/tui_smoke.py ./target/debug/softwarefactory
```

See [docs/VALIDATION.md](docs/VALIDATION.md) for recorded evidence, and [docs/ACCEPTANCE.md](docs/ACCEPTANCE.md) for the full plan contract and external pilot milestones. The interactive graph, editable flows, generalized model routing, scheduling and Android pilot are intentionally future scope.
