# Software Factory terminal control panel

Run `softwarefactory --project /absolute/path/to/project tui` (or omit `tui`). The Rust/Ratatui interface opens a local control panel; it does not start agents simply by opening. The existing wizard is available through **E** or `tui --setup`.

## Screens and keys

| Screen | Purpose |
| --- | --- |
| Home | Product objective, active cycle, prerequisites, next action and board synchronization health |
| Workflow | Current stage, call/time budget, blockers and recorded activity |
| Tasks | Local Kanban view, current agent and cycle; Left/Right selects columns, Up/Down selects tasks |
| Review | Exact proposals/results needing a human decision; Enter opens criteria, bounded code/guidance diffs, reviewer findings, check outcomes, evidence references and recorded decisions |
| History | Saved workflows and cycle learning; selecting a workflow retains its identity |
| Issues | GitHub intake, queued repairs, source changes, linked workflows and fix/regression evidence |
| Settings | Current profile, budgets, reviewer count and board configuration; V lists installed versions, U checks published update metadata |

Tab/Shift+Tab moves between screens; 1–7 selects one directly. Enter opens details, Esc returns, Up/Down scrolls details. `/` filters tasks, Enter applies and C clears. On narrow terminals fewer board columns appear at once; every column remains reachable. Below 42×12 the panel asks for more space while retaining safe close behavior. `NO_COLOR` disables control-panel colors; statuses retain text labels.

**N** opens an objective and optional cycle-limit form, checks prerequisites, and starts the full workflow. **R** runs a saved workflow; **B** resumes a recoverable blocked/no-opportunity workflow. The start/run confirmation states that configured review packets and explicitly enabled Notion board updates may be written. Every feature and harness decision still requires the configured human decision source.

**Space** pauses scheduling after the current bounded operation completes. **Q** closes the panel after that operation and restores the terminal; it does not leave a hidden background runner. **X** requests cancellation independently of the busy worker. Unknown termination remains blocked until reconciled. The interface remains responsive while commands run. One runner owns a project transition at a time; use one synchronization host for a workflow.

**P** probes the configured runtime/review capabilities and optional board schema/access. Ready/Missing/Failed/Not checked are distinct. Successful probe results are tied to the profile and adapter bytes and expire after five minutes. Checks are marked configured but not executed until there is an approved candidate. A raw Codex/Claude executable does not implement the bridge protocol; a compatible production bridge and its authentication remain required.

**O** opens a recorded Notion HTTPS review link. The TUI displays exact candidate criteria, bounded previews of the selected run’s diff artifacts, reviewer findings, harness comparison summaries and check evidence references; it does not upload local artifacts or mint approvals. Feature acceptance does not authorize merge/release. Harness authorization does not install a new configuration.

## Setup and settings

The wizard retains preview-before-apply and safe cancellation. It now includes executable argument arrays, workflow time/call limits, command timeout, allowed paths, Notion database/data-source IDs, board write enablement, an optional property-map JSON file, review kind and credential environment-variable name. Existing custom mappings and arguments are preserved on repeat setup. A changed project path is used by the dashboard only after successful apply.

Configure the Notion review command using the installed `bridges/notion_review.py` (or Python plus its absolute path as an argument), `review.kind = "notion"`, authorized human Notion user IDs, and launch-environment `NOTION_TOKEN` / `NOTION_PAGE_ID`. Tokens are not entered into the TUI or written into the profile. Use the same reviewed setup service for advanced JSON profiles.

Version checks in Settings read metadata only. Installation and project adoption remain explicit existing CLI operations (`update --apply`, `project-update --apply`), each with its existing preview/recovery behavior. Active runs keep their pins.

## Notion board connection

The board adapter uses an existing database/data source. Share both that database and the review-packet parent with the integration. Create a board view grouped by Status. Required content capabilities are read/insert/update; review decisions also need comment read and human user identity access. Configure properties below, then press P or run `softwarefactory board --probe`. The probe returns resolved property IDs; an optional mapping file can use those stable IDs instead of names. A renamed/unavailable mapping stops synchronization with a specific error.

| Property names | Type |
| --- | --- |
| Name | Title |
| Status | Status: Discovery, Needs approval, In progress, Ready for review, Blocked, Done, Deferred, Cancelled |
| Record key, Project ID, Workflow ID, Type, Stage, Current agent, Summary, Blocker, Next action, Artifact revision | Rich text |
| Cycle, Iteration, Event sequence | Number |
| Event time, Last synced | Date |
| Review packet | URL |

The minimal profile addition is `kanban`, containing `enabled`, `database_id`, `data_source_id`, and optional `properties` mapping semantic names to Notion names/IDs. `examples/notion-kanban.json` documents that object. Set IDs and mappings through previewed setup; enable writes explicitly. Schema or board creation is not automatic, so existing databases and user-defined properties are preserved.

**S** explicitly synchronizes the board. Enabled board sync also runs during TUI/CLI continuous workflows. `softwarefactory board` reports the queue; `board --sync` retries it. Local cards work without Notion. API calls use the documented [data source schema](https://developers.notion.com/reference/retrieve-a-data-source) and [page property updates](https://developers.notion.com/reference/patch-page).

A durable per-destination queue coalesces each card's latest saved state with increasing sequence numbers. Up to eight eligible cards are processed per pass, with a 60-second pass budget. A blocked card retains its own retry deadline so other cards can update; a rate limit backs off the whole connection. Every update is read back; failures retain the pending event, last success and retry deadline. HTTP Retry-After is honored with at least 60 seconds between failed passes. Lost creation responses query the original stable key. If no original can be located, synchronization remains blocked instead of blindly inserting a duplicate. Restore deleted/archived cards; reconcile duplicates or a newer remote sequence before retrying. Human notes and unmapped properties are untouched. Only use one sync writer per workflow; Notion does not provide an atomic unique-key upsert here.

Missing approvals pause dependent work. A failed progress sync does not revoke already verified authority; authorized local work can continue within its budgets, with the pending/error state visible in Home. Polling alone does not grant authority, and dragging a card grants none.

## Validation and limits

The local interface, queue and adapter are tested with synthetic projects, real terminal keystrokes and offline Notion API responses. Live Notion access/write permissions and real provider authentication are **not verified** until you configure them. No board, notification, remote message or production feature was created during implementation. A graphical workflow editor, background daemon and arbitrary per-role provider routing are outside this TUI increment.


## GitHub issues

Press **7** for Issues, then **G** to enter `OWNER/REPO` and an optional label. Tab switches fields, Ctrl+U clears, Enter scans and Esc cancels. Scanning uses the authenticated GitHub CLI and saves local tasks without dispatching agents. A failed scan keeps the prior queue visible; G retries with the retained input.

Press **R** to run the saved issue queue after confirming configured review/board synchronization. The worker polls GitHub every 30 seconds between bounded stages, including when idle. **Space** pauses scanning and execution after the current stage. **Q** closes the TUI and stops scheduling further work. There is no background daemon.

Up/Down selects an issue; **Enter** opens its original report status, workflow/run links, mandatory fix/regression criteria and check outcomes. **W** opens its Workflow view. **/** searches issues and **C** clears the search. The existing Tasks and Review screens retain full linked evidence and human-decision details.

After resolving a blocker, **B** resumes the selected issue and its monitor. **X** pauses scheduling and requests cancellation of active work. After a finished/stopped attempt, **T** queues an eligible issue for a new attempt with fresh approval; scan again first if the report or eligibility changed. See [the issue loop guide](GITHUB-ISSUE-LOOP.md) for evidence and source-change semantics.
