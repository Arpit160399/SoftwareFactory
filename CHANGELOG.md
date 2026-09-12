# Changelog

## Unreleased

- Add an Issues TUI tab with repository/label scan, continuous repair monitoring, pause/resume, cancellation, retries and fix/regression evidence details.

- Add a GitHub issue queue and foreground repair loop with paginated intake, optional label selection, duplicate prevention, durable handoff recovery and explicit retries.
- Require issue-fix and regression criteria with executable journey evidence before implementation; reuse independent review, repair iterations, human acceptance and retrospective gates.
- Preserve original issue snapshots and report changed source, blocked work and unresolved outcomes without treating GitHub closure as repair evidence.

## 0.2.0 — 2026-09-08

- List installed releases, the running binary version and the active launcher version with `versions`.
- Check published stable GitHub releases with `update --check`, then download, verify and activate them with `update --apply`. Installation prefixes are detected automatically.
- Install local builds with `update --local`, or select an installed/published release with `update --to VERSION`.
- Build native release assets for Apple Silicon macOS, Intel macOS and x86-64 GNU Linux through a tag-triggered workflow that prepares a draft for publication.
- Retain older releases for explicit rollback. Installation is serialized; release staging and launcher replacement avoid partial activation.
- Record and verify SHA-256 manifests for new releases. Existing 0.1.0 bundles remain selectable with their missing-manifest status disclosed.
- Reject changed builds under an existing version, incomplete releases and conflicting launchers.
- Adopt the running release in one project's configuration with `project-update`, using the saved profile and reversible setup transactions. Existing runs and other projects keep their pins.

Published updates use GitHub Releases over HTTPS and require GitHub SHA-256 asset digests. Publisher signatures, notarization and active-run/schema migration are not included.

## 0.1.0 — 2026-09-08

- Initial project setup wizard, workflow coordinator, command bridges, approvals, recovery and local versioned installation.
