# Run the whole product workflow in a loop

The outer coordinator repeats **research → opportunities → product proposal → build approval → planning → implementation → checks → independent review → human acceptance → retrospective → optional harness decision → next discovery cycle**. Failed feature reviews return to planning within the current cycle. Every cycle has new discovery, feature and retrospective identities; previous findings, evidence references and learning summaries become context for the next discovery pass.

Configure the project through the Rust TUI first. Then:

```sh
softwarefactory --project /path/to/project workflow start "Improve the product's most important unmet user need"
softwarefactory --project /path/to/project workflow run WORKFLOW_ID
```

`start` returns the saved ID. `run` remains active at human gates and continues when a separately verified decision arrives. To synchronize review packets and poll the configured review source automatically, explicitly enable that behavior:

```sh
softwarefactory --project /path/to/project workflow run WORKFLOW_ID --review
```

`--review` can create records in the configured external review system. Every polled claim is independently checked against its human author, project, feature and exact artifact revision. Prior-cycle approvals do not authorize another feature. Acceptance does not grant merge or release authority.

For bounded sessions or scripts:

```sh
softwarefactory --project /path/to/project workflow start "Improve onboarding" --max-cycles 2
softwarefactory --project /path/to/project workflow run WORKFLOW_ID --until-wait
softwarefactory --project /path/to/project workflow step WORKFLOW_ID
softwarefactory --project /path/to/project workflow status WORKFLOW_ID
softwarefactory --project /path/to/project workflow list
softwarefactory --project /path/to/project workflow resume WORKFLOW_ID
softwarefactory --project /path/to/project workflow stop WORKFLOW_ID --reason "Pause this product effort"
```

Without a cycle cap, the coordinator keeps cycling until stopped, a budget is exhausted, a failure requires recovery, or discovery finds no actionable opportunity. A no-action/defer finding completes its retrospective and pauses for new direction; `resume` explicitly starts another discovery cycle. A focused research request from the product manager becomes the next cycle's question. Reaching a cycle cap completes the session; it does not claim that every product goal has been achieved.

The project profile's elapsed-time and agent-dispatch budgets apply across the entire session. Polling and restarting do not replenish them. Existing submitted work can still be looked up and cancelled after the budget is exhausted. Fresh dispatches and checks are blocked at the elapsed deadline; running commands are interrupted and require reconciliation. Approval waiting counts toward elapsed time. The budget measures elapsed time and agent dispatches, not provider token cost.

Progress is saved under `.product-workflow/workflows/WORKFLOW_ID/`. IDs are saved before child work is created. An interrupted operation looks up the same job instead of submitting a replacement. Running the same ID resumes this saved progress; `resume` clears a recoverable blocked state after its cause is resolved. A stop request is durable and must confirm any child termination before the session is reported stopped. Workflow execution remains pinned to its original core release and repository location.

Retrospectives must cite inspected artifacts. Proposed harness changes need real paired development and held-out trial evidence, frozen by the comparison service. Humans can authorize adoption or defer the candidate while retaining the current harness. A verified `harness_defer` claim can arrive through review polling or `defer-harness CANDIDATE_ID /path/to/decision.json`; `adopt CANDIDATE_ID /path/to/decision.json` handles `harness_adopt`. Applying a release or configuration change is still an explicit setup/update operation; an adoption decision alone does not install arbitrary candidate artifacts. The following cycle receives the candidate and its disposition as context.

The existing `run RUN_ID` command operates one feature. **`workflow run WORKFLOW_ID` operates the whole repeating workflow.** The TUI now provides setup, a responsive dashboard, task/review screens and the same continuous runner controls; see [the TUI guide](TUI.md). These commands remain available for scripts.

## Command runtime contract

Runtime executables receive one JSON request on stdin and return one JSON response on stdout. The runtime must support all seven roles, distinct contexts, protected/read-only scopes, stable job IDs, lookup and cancellation. Discovery requests now include `workflow_id` and `workflow_context` with cycle number and prior-cycle references. A retrospective dispatch uses role `harness-reviewer` with discovery/feature artifacts and an explicitly scoped artifact directory. It returns a summary, evidence references, optional next question and one of `no_change`, `deferred`, or `candidate`; a candidate also includes a valid harness comparison input.

Review bridges used with `--review` support `fetch_decisions` with project ID, feature ID, artifact revision and accepted action names. They return `{"decisions": [...]}`; this supplies claims only and does not replace `verify_decision`.

Codex CLI, Claude Code or another CLI can be wrapped behind this command protocol. A raw interactive CLI command is not itself a compatible bridge. This change supplies the coordinator and synthetic end-to-end bridge; it does not supply or certify a production Codex/Claude adapter. See [ADAPTER-PROTOCOL.md](ADAPTER-PROTOCOL.md) for the base protocol and [VALIDATION.md](VALIDATION.md) for live-pilot limits.
