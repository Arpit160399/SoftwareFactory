# Terminal workflow contract

## Business sources
`multi-agent-product-workflow-plan.md`, `docs/WHOLE-WORKFLOW.md`, `docs/NOTION-KANBAN-PREREQUISITES.md`, `docs/GITHUB-ISSUE-LOOP.md`, `src/issues.rs` and `src/engine.rs` own workflow/approval semantics. The TUI invokes those services and never mints approvals, runs raw interactive agent commands or silently activates updates.

## Canonical UI Map
| Capability | Canonical owner | Source of truth | Allowed variants | Verification |
| --- | --- | --- | --- | --- |
| Form | src/setup_tui.rs, src/tui.rs modals and setup/issues services | Profile validation, SetupPlan and issue intake contract | setup / reconfigure / GitHub scan | tests/tui_smoke.py |
| Navigation | src/tui.rs App and draw | DESIGN.md | seven tabs, modal details | tests/console.rs and tests/console_smoke.py |
| Table Selection | src/tui.rs list/indices | saved work item identities | task/status filter, workflow history, issue number selection | tests/console.rs |
| Scrollbar | Ratatui ListState and Paragraph scroll | content panel bounds | list / detail | terminal smoke and narrow buffer test |
| Toast | src/tui.rs footer notice | worker Event::Message | success / failure / pending | console smoke |
| CRUD | src/console.rs worker and setup services | existing engine/approval contract | create workflow / preview config / scan and retry issues | tests/console.rs |
| Async work | src/console.rs worker | saved coordinator state | run / pause / stop / sync / issue monitoring | tests/console.rs |

English locale. No dates are edited. A graphical browser, DOM, pointer gestures and CSS control geometry are inapplicable to this Rust terminal surface. Unicode input, named controls, text statuses and visible keyboard focus are required; screen reader conformance is not claimed for an alternate-screen terminal. No token cost or completion percentage is invented.

## Interaction ledger
- Start: objective and optional cycle limit → prerequisites/probe → saved workflow → active worker; errors preserve local history and pause scheduling.
- Run/Resume: exact selected workflow → existing service validation → next bounded transition; approval source remains authoritative.
- Pause: finish current bounded operation, save state, schedule no next stage. The UI remains usable while pausing.
- Close: request worker shutdown, finish current operation, restore terminal; there is no hidden background service.
- Stop: durable stop request delivered separately from the busy worker. Unknown termination remains blocked; never display cancellation as confirmed merely because requested.
- Review: inspect exact card/proposal/check metadata; open a validated Notion HTTPS packet link. No approval hotkey bypasses source verification.
- Setup: preview exact changes then apply; cancelled edits leave files unchanged. Saved config/active runs retain current service pin semantics.
- Board: stable record key and monotonic local sequence; retries query existing cards before insertion. Ambiguous create, missing prior card or duplicate key remains pending for reconciliation. Only configured properties are updated; notes stay user-owned.
- Sync failures: keep local work visible, persist error/queue/backoff, report last success. Already-approved work can continue within its budgets.
- Settings updates: available version check reads metadata only; installation/adoption uses existing explicit CLI operations.

## Recovery and evidence
Test empty setup, configured home, running/paused/blocked workflow, review waits, search clearing, long content, narrow terminal, interrupted sync, duplicate-submit prevention and terminal restoration. Static audits supplement actual pseudo-terminal tests; they cannot certify provider behavior or live account permissions.


## GitHub issue controls

The Issues tab uses the same list, detail, modal, footer and worker owners as the existing screens. `src/issues.rs` owns task identity, frozen inputs, retry eligibility and issue-to-workflow transitions; the UI does not duplicate those rules. `src/console.rs` owns periodic scheduling, and the existing workflow/review services own acceptance and regression checks.

- Scan (G): repository and optional label → validate fields → background read → retained issue list and scan result. No agents dispatch. Errors preserve typed inputs for reopening and previously saved tasks; field errors remain inside the form.
- Run (R): saved intake → confirmation of monitoring and configured external review/board writes → readiness/probe → bounded issue transitions. The visible monitor state remains active even when the queue is idle. The same worker owns manual and issue runners, preventing concurrent scheduling.
- Pause (Space) and close (Q): complete the current bounded operation, save progress and stop scheduling scans/transitions. No hidden monitor survives closing the TUI.
- Resume (B): selected blocked issue → existing workflow resume → issue monitoring. Retry (T): eligible terminal attempt → confirmation → existing issue retry service → queued list, with prior attempt history retained and fresh approval required.
- Stop (X): pause scheduling first and send the durable child cancellation separately from the busy worker. The queue must not start its next task while stopping the current child.
- Details (Enter): issue source and changed-report status, linked workflow/run, mandatory fix/regression criteria and current check outcomes. W opens its Workflow view. Tasks and Review retain their existing evidence and human-decision views.
- List refresh restores selection by issue number, not row index. Failed reads retain cached rows with an explicit error. `/` searches locally and C clears. At short heights compact rows keep the selected issue visible; narrow forms preserve field and action access.

Verification: `tests/console.rs` covers read models, selection and narrow rendering; `tests/issues_tui_smoke.py` exercises actual keyboard scan, invalid input, read failure/retry, confirmations, repair iterations, approval waits, stop/retry, idle monitoring, narrow forms and terminal restoration alongside the existing console smoke.
