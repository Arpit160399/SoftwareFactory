# Notion Kanban: behavior and prerequisites

Status: the local task projection, queue and Notion card adapter are implemented and tested offline. Live workspace configuration and permissions remain to be validated. See [the TUI guide](TUI.md) for the actual configuration, property schema and recovery controls. This document records the original requirements and operator prerequisites.

## Intended user experience

Use a Notion database with a board view grouped by Status. Show work items for discovery, proposed features and harness improvements, each linked to its project, whole-workflow ID and cycle. Feature cards persist across repair iterations; another cycle or a distinct feature gets a fresh identity. Discovery remains visible even when it produces no feature. Harness changes have their own decision and card rather than marking an accepted product feature unfinished.

| Board column | Meaning |
| --- | --- |
| Discovery | Research, opportunity synthesis or product planning is underway |
| Needs approval | A specific proposal or harness candidate requires a human decision |
| In progress | Planning, implementation, checks, review or replanning; Stage gives the detail |
| Ready for review | Automated feature review passed; human acceptance is needed |
| Blocked | Missing information, failed integration, uncertain job state or exhausted budget; reason and recovery action shown |
| Done | Feature accepted, discovery concluded, or harness decision recorded; Outcome identifies which |
| Deferred | No actionable opportunity or a human deferred the work/change |
| Cancelled | Stop/cancellation confirmed; a request alone is not completion |

Each card shows a plain-language summary, current stage/agent, cycle/iteration, latest verified outcome, blocker if any, next action, last event time and last successful sync time. Details link to the immutable proposal, approval packet, relevant evidence and related work items. A feature can be Done while its retrospective continues as a separate item. Done does not mean merged or released.

Update cards on meaningful saved transitions and new blockers/results. Use one stable record key to update the same card across retries/restarts. Coalesce redundant updates; do not publish raw tool logs, credentials or full prompts. Keep user-written notes separate from machine-managed properties. Board visibility is the initial update mechanism; notifications, mentions, email and Slack messages are not enabled by this requirement.

## Prerequisites and ownership

| Requirement | What must be supplied or verified | Current readiness |
| --- | --- | --- |
| Project | Stable project ID, repository, product brief, allowed paths and relevant guidance | Supported by profile/setup |
| Runtime | Installed and authenticated command bridge supporting all seven roles, required reasoning, separate contexts, scope enforcement, lookup and cancellation | Generic protocol exists; production Codex/Claude bridges still needed |
| Verification | Actual required check commands, fixtures and readable evidence destinations | Command checks supported; project-specific checks must be configured |
| Notion destination | Selected workspace, database, its task data source and a board view grouped by Status; selected review-packet parent page | User must choose destination; existing board/schema setup is required; card sync is implemented |
| Connection | A Notion integration/connection shared with both the task database and review parent | Must be configured and verified in the target workspace |
| Permissions | Read, insert and update content for task sync; comment read and user identity access for approval verification | Existing bridge covers review pages only; board schema/access preflight is implemented; write access needs a live test |
| Credentials | `NOTION_TOKEN` supplied through the runner's environment; no secret in profile, repository or card | Supported convention; credentials not configured by this document |
| Destination IDs | Existing `NOTION_PAGE_ID`; new database/data-source/view IDs and property mapping | Supported in the optional `kanban` profile configuration |
| Human authority | Authorized Notion human user IDs; build, acceptance and harness adoption/deferral remain distinct decisions | Supported decision contract; real reviewers must be supplied |
| User access | Intended viewers can open the board, review packets and shared evidence links | Needs workspace verification; local filesystem paths are not remotely readable links |
| Execution host | Machine running Software Factory, Python 3 for bundled bridges, network access and a continuously running workflow process | Local runner exists; scheduled/background hosting is not configured |
| Limits and scope | Elapsed/call budgets, optional cycle cap, enabled project board sync and selected destinations | Budgets supported; board sync requires explicit profile enablement |

Notion content access and comment access are separate capabilities. Grant comment insertion only if an explicitly enabled feature needs to post comments; updating board properties does not require posting notification comments. See [connection capabilities](https://developers.notion.com/reference/capabilities) and [working with comments](https://developers.notion.com/guides/data-apis/working-with-comments).

## Proposed database contract

Map stable property IDs in adapter configuration so display labels can be renamed. Keep Notion-specific IDs out of the reusable engine.

| Property | Type | Purpose |
| --- | --- | --- |
| Name | Title | Human-readable work item |
| Status | Status | Board column above |
| Type | Select | Discovery, Feature, Harness improvement |
| Project ID | Rich text | Separate multiple projects |
| Workflow ID | Rich text | Link all cycles within one workflow |
| Cycle | Number | Whole-workflow cycle |
| Record key | Rich text | Unique local identity used for replay-safe upsert |
| Run ID | Rich text | Feature identity when applicable |
| Stage | Select | Current coordinator stage |
| Current agent | Select | Research, opportunities, PM, planner, implementer, reviewer, harness reviewer, or none |
| Iteration | Number | Feature repair iteration when applicable |
| Summary / Outcome | Rich text | Verified progress and result |
| Blocker / Next action | Rich text | What prevents progress and who needs to act |
| Reviewer | People | Assigned human; assignment alone grants no authority |
| Artifact revision | Rich text | Exact proposal/candidate under review |
| Review packet / Evidence | URL | Accessible approved links; multiple links can be in page content |
| Event time / Last synced | Date | Freshness of local work and remote projection |
| Event sequence | Number | Prevent older queued updates overwriting newer state |

A board is a view of database records grouped by a property, as described in [Notion's board documentation](https://www.notion.com/help/boards). Database containers and their data sources have distinct API IDs; permissions are granted at the database level. See [Notion's database/data-source model](https://developers.notion.com/guides/get-started/upgrade-faqs-2025-09-03).

## Readiness checks before enabling board sync

1. Validate configuration, runtime capabilities and required checks without starting product work.
2. Read the chosen database/data source, property types, status choices, review root and authorized human identities. Reject missing access or incompatible mappings with an actionable message.
3. Present the destination and exact planned schema/view changes. Never silently replace an existing database or repurpose user-owned properties. Board creation and synchronization need explicit enablement for the selected destination.
4. In an explicitly authorized test destination, create one labeled test card, update it through sample statuses, and read the changes back. Replay the same event and restart the runner; confirm there is still exactly one card. A timeout after creation must query the stable key before retrying insertion.
5. Confirm an exact human decision is accepted and a bot, wrong revision, wrong project or card-status change grants no authority.
6. Test temporary network failure, rate limiting, deleted/archived cards, revoked access and concurrent updates. Retain a durable local sync queue, honor retry delays and display pending/error sync state locally. Once connected, mark freshness and replay in order without stale overwrites or silent duplication.
7. Verify evidence links work for the intended user. Link/export only the selected shareable evidence; do not upload arbitrary workspace files.

The coordinator remains the source of execution state. A manual drag changes the display, not permission to run code. An unavailable approval source blocks work that needs a new decision. Already-authorized work can continue within its limits while progress updates are queued; the local runner must make stale Notion synchronization visible.

## Implemented behavior and live checks remaining

- Extend the reviewed Notion adapter configuration with destination IDs, stable property mappings and an explicit sync setting.
- Implement database-card creation/update/lookup and schema/view validation alongside the existing immutable approval packets.
- Connect saved discovery, feature, check, retrospective, approval, blocked and stop transitions to a durable sync outbox; add replay-safe recovery and state ordering.
- Add the prerequisites/readiness results to setup and doctor, including clear distinction between review-page readiness and Kanban readiness.
- Pass offline contract tests and the live authorized test-board checks above before labeling the integration ready.

The implementation provides these local services; the authorized live board test and production runtime connection remain operator prerequisites. No Notion database, credentials, notifications or production runtime were created by this work.
