# Command adapter protocol 1

Every adapter invocation receives one JSON object on stdin and writes exactly one JSON object to stdout. The executable and arguments come from the reviewed profile. There is no shell interpolation. Nonzero exit, invalid JSON, timeout, output above 16 MiB, identity mismatch and unknown submission are errors, never success. Requests/results are local project artifacts. The runtime bridge is trusted executable code: it must enforce capabilities, restrict agent tools, protect `.product-workflow`/`.git` and honor scopes; the coordinator does not sandbox an arbitrary executable by itself.

## Runtime

`{"operation":"capabilities","protocol_version":1}` returns:

```json
{
  "protocol_version": 1,
  "roles": ["planner", "implementer", "reviewer"],
  "high_reasoning_models": ["your-verified-model"],
  "idempotent_dispatch": true,
  "lookup": true,
  "cancel": true,
  "enforces_read_only": true,
  "enforces_scope": true
}
```

Only advertise a capability after verifying it against the underlying runtime. A model string or prompt instruction does not enforce a sandbox or prove reasoning support.

`dispatch` includes project/run IDs, an idempotency key, fresh context ID, role, model/reasoning requirement, permissions, allowed/protected paths, root, artifact directory, approved proposal, pinned profile/context/templates, original and current guidance, source snapshot, previous iteration and budget. Roles return:

```json
{
  "status": "completed",
  "context_id": "the-exact-request-context-id",
  "effective_model": "the-actual-model",
  "effective_reasoning": "high",
  "result": {"role": "planner", "result": {"summary": "..."}}
}
```

`src/engine.rs` defines the full serialized role schemas (`PlannerResult`, `ImplementerResult`, `ReviewerResult`). `examples/fixture_bridge.py` demonstrates complete responses. Implementer diff/guidance references must resolve to nonempty files inside the current run directory. Compute `candidate_revision` using `softwarefactory --project ROOT snapshot`; the revision is SHA-256 over sorted, compact JSON mapping source-relative file names to SHA-256 hashes. The snapshot excludes `.git`, `.product-workflow`, `target`, and `node_modules`; individual source files over 64 MiB require a different repository adapter. Symlink targets are represented explicitly. Directly configured adapter scripts and file arguments are independently pinned.

`lookup` uses the same idempotency key and returns the original completed response, a pending response, or `{"status":"unknown"}`. Never create a job in response to lookup. Persist a submission before side effects and preserve the original context/model/evidence. The coordinator deliberately leaves unknown work blocked; an adapter-specific operator must reconcile it before retry. `cancel` returns `{"status":"cancelled"}` or `{"status":"completed"}` only after termination/completion is established. Other responses keep cancellation incomplete.

Discovery uses the same dispatch/lookup protocol with roles `research`, `opportunities`, `product-manager` and read-only permissions. Research returns `findings` with claim/source/observed_at/confidence/limitations; opportunities returns an `opportunities` array of at most three; PM returns a valid `proposal` or `disposition` of `no_action`, `defer` or `research`.

## Review

Capabilities must report `protocol_version: 1` and `attributable_decisions: true`.

`submit_packet` carries `idempotency_key` and immutable `packet`. Repeated calls must reconcile the same packet, with a stable `reference` returned. External writes happen only on the explicit `sync-review` command.

`verify_decision` receives a `claim` with `id`, `project_id`, `run_id`, `actor`, `decided_at`, `artifact_revision`, `source`, `action`. Return `verified: true`, an exactly matching authoritative `claim`, `actor_type: "human"`, and nonempty `source_evidence` only after reading the trusted human decision. Do not echo unverified input. The core validates project/run/action/stage/revision separately. Previous saved decisions are reverified before further execution. Unavailable provenance pauses dependent work.

The bundled Notion bridge uses immutable packet pages and human comments. It is based on Notion's [comment listing](https://developers.notion.com/reference/list-comments), [comment metadata](https://developers.notion.com/reference/comment-object), and [page creation](https://developers.notion.com/reference/post-page) APIs. Network/account conformance still requires a live pilot.

## Checks

`check` includes project/run, iteration, idempotency key, stable check ID, category, linked criteria and exact candidate revision. Return:

```json
{
  "check_id": "journey-primary",
  "revision": "exact-candidate-sha256",
  "outcome": "passed",
  "environment": "actual environment",
  "fixture_version": "actual fixture or null",
  "visible_outcome": "observed result",
  "persisted_outcome": "observed saved result"
}
```

Outcomes are `passed`, `failed`, `blocked`, `not_run`. Execution state is recorded separately by the coordinator. Check metadata, output and errors are retained; no aggregate score overrides a required failure. The native bridge adds actual command exit status, timing, platform and log reference. iOS commands should also produce an `.xcresult` bundle at a configured artifact path and report actual simulator/fixture metadata.

`lookup_check` must return the saved exact-revision result or `{"status":"unknown"}` without repeating execution. `cancel_check` must confirm `cancelled`/`completed`; unavailable or unknown cancellation keeps the feature blocked and retains the one-active-feature restriction. Completed passed/failed evidence is cached across recovery. Shared resources use a named machine-local lock.

The native bridge intentionally does not restart an interrupted unknown command. It supports synchronous commands; wrappers launching independent remote jobs need a bridge with authoritative remote lookup/cancellation.

## Fixtures

`examples/fixture_bridge.py` requires `.softwarefactory-synthetic-fixture` in the selected temporary repository. Its model and approval identities are synthetic. It implements a real failing-first repair loop against `result.txt`, with saved receipts and evidence, to exercise package behavior. Never use it as the review adapter of a real product.
