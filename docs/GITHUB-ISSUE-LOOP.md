# GitHub issue repair loop

Software Factory can poll a GitHub repository, create durable local issue tasks, and pass each task through the existing discovery, product proposal, technical planning, implementation, independent checks/review, human acceptance and retrospective workflow. Failed checks or review findings return to planning. Only one issue workflow owns a project's source at a time.

## Use the terminal interface

Open the TUI and press **7** for Issues. **G** opens repository/label input and scans; **R** runs the queue; **Space** pauses. **Enter** opens issue details and fix/regression evidence, **B** resumes blocked work, **X** stops an active attempt and **T** retries a finished/stopped issue. **Q** closes the interface and stops scheduling. See [the TUI guide](TUI.md).

## Configure and start

Use a project configured through `softwarefactory setup` with discovery enabled, a compatible command runtime, a human review bridge and real executable checks. Include checks for the reported outcome and affected existing behavior. A raw model executable is not a compatible runtime bridge; see [the adapter protocol](ADAPTER-PROTOCOL.md).

Install/authenticate GitHub CLI (`gh`) with read access to the selected repository. The reader uses [`gh api --paginate --slurp`](https://cli.github.com/manual/gh_api) against github.com and reads every page of open issues, excluding pull requests. GitHub Enterprise is not supported by this intake command. The repository argument is explicit; select the matching local checkout with `--project`.

```sh
# Queue matching issues without dispatching any agents.
softwarefactory --project /path/to/project issues scan --repo OWNER/REPO --label bug
softwarefactory --project /path/to/project issues status

# Run until the next human gate or idle queue.
softwarefactory --project /path/to/project issues run --repo OWNER/REPO --label bug --until-wait

# Continue polling in the foreground and synchronize/poll the configured review source.
softwarefactory --project /path/to/project issues run --repo OWNER/REPO --label bug --review --poll-seconds 30
```

Omitting `--label` queues all open issues. Labels select intake; they never grant build or acceptance authority. `--review` explicitly permits writes to the configured review surface, which may be Notion. GitHub intake itself only reads issues: it does not comment, close issues, push source, open PRs, merge or release. Without `--review`, submit verified decisions through the existing `decision RUN_ID FILE` command and continue the loop.

The monitor runs while this command is running. It checks GitHub between bounded workflow transitions at the selected interval (1–60 seconds); a running role/check can delay the next scan. It prints task changes, rather than repeating unchanged status. This is a foreground worker, not an installed background service. Ctrl+C stops the worker; restart the same command to reconcile saved work. To cancel a child job explicitly, use the workflow stop command below.

## What counts as a verified repair

Every issue proposal must contain both mandatory criteria:

- `ISSUE-FIX`: concrete reproduction and the observable expected result.
- `ISSUE-REGRESSION`: the existing behavior and affected journeys that must remain correct.

Both criteria require nonempty `check_ids` referencing configured checks, and a required journey linking each criterion to executable evidence. Mandatory project checks remain required. The coordinator rejects proposals missing this contract before creating an implementation run. The technical planner maps both criteria to tasks and checks; the implementer receives the frozen proposal and original issue snapshot. Independent review evaluates every mandatory criterion and all required checks on the exact candidate. Failed checks return the attempt to planning; changed candidates require fresh checks and review.

The coordinator enforces evidence presence, identity and required-check outcomes. The runtime, check commands and human reviewer must still ensure those checks actually cover the expected product behavior. A build alone cannot establish regression safety, and a passing suite does not prove absence of all possible regressions. Use representative reproduction, persistence/recovery and affected-caller coverage, and record coverage limits.

`awaiting_acceptance` means the candidate has passed the engine's checks and independent review but awaits human acceptance. `accepted` means the completed issue workflow contains an accepted feature; its evidence and candidate are historical records. `unresolved` means discovery/retrospective ended without an accepted repair. Cycle limits, no-action decisions, GitHub closure and cancelled attempts never imply a fix.

## Recovery and changed issues

State is stored under `.product-workflow/issues/queue.json`, linked to ordinary workflow/run records. Repository identity and issue number prevent duplicate tasks on repeat scans. The child ID and original issue snapshot are saved before child creation, allowing interrupted handoffs to resume without allocating a second workflow. Each issue uses one outer cycle with the existing repair dispatch/time budgets; repair iterations continue within that cycle.

Queue status shows the issue URL, workflow/run IDs, eligibility, state and block reason. Full task/run details remain available through `workflow status WORKFLOW_ID` and `status RUN_ID`. Completed records stay linked so an open issue is not repeatedly rebuilt. Human decisions and earlier attempt history never transfer to a retry.

```sh
softwarefactory --project /path/to/project issues step
softwarefactory --project /path/to/project workflow status WORKFLOW_ID
softwarefactory --project /path/to/project workflow resume WORKFLOW_ID

# Stop the monitor first if it is running, then cancel the active child.
softwarefactory --project /path/to/project workflow stop WORKFLOW_ID --reason "Replan updated issue"
softwarefactory --project /path/to/project issues scan --repo OWNER/REPO --label bug
softwarefactory --project /path/to/project issues retry ISSUE_NUMBER
softwarefactory --project /path/to/project issues run --repo OWNER/REPO --label bug --until-wait
```

Later title/body edits set `source_changed`; an active workflow continues against its original frozen report and scope. Review the change and explicitly stop/retry when new requirements need a new proposal. Closed issues and removed labels make queued tasks ineligible. Active work is preserved until explicitly stopped, and previously completed/reopened issues require an explicit retry. Retry requires an eligible issue and a terminal earlier workflow; all previous attempt IDs remain in the queue record.

A blocked workflow stops the foreground runner with a nonzero exit status. Resolve the recorded cause and resume that workflow; existing dispatch/check recovery uses the same saved idempotency key. A failed, malformed, duplicate-page or timed-out GitHub scan retains the previous queue and records the error. The runner exits with an error and starts no new task from that failed scan. Refresh successfully before new intake; already-started work can still be advanced explicitly with `issues step`. A manually started whole workflow must finish or be stopped before another issue starts.

## Local validation

Automated fixtures exercise the actual CLI/reader argument contract, pagination, label selection, PR exclusion, repeat scans, closure, failed scans, interrupted reservations, immutable issue input, fresh retry IDs, missing evidence contracts, no-action outcomes, stale candidate rejection, and a full repair where the targeted check passes but a regression check fails. These tests do not invoke a live AI provider or modify GitHub. Live operation requires the project's runtime, checks, human reviewers and GitHub access to be configured and validated.
