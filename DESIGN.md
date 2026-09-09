---
version: alpha
name: Software Factory terminal
description: keyboard product workflow control panel
colors:
  primary: '#6bcad7'
  background: '#172331'
  text: '#dce5ed'
  muted: '#9daebe'
  warning: '#f5c46e'
  danger: '#f29191'
typography:
  mono:
    fontFamily: 'ui-monospace, monospace'
omitted:
  - section: rounded
    reason: Terminal cells and square panel borders have no radius.
  - section: spacing
    reason: Layout uses terminal rows and columns rather than CSS lengths.
components:
  panel:
    backgroundColor: '{colors.background}'
    textColor: '{colors.text}'
  navigation:
    textColor: '{colors.primary}'
  supporting:
    textColor: '{colors.muted}'
  warning:
    textColor: '{colors.warning}'
  error:
    textColor: '{colors.danger}'

---

## Overview

### Creative North Star
A workshop job traveler: every item shows where it is, what passed, and what the operator needs to do next. The signature is a visible Discover → Decide → Build → Verify → Learn rail, anchored to a real cycle and its remaining budget.

### Product context and register
Developers operating a local product workflow. English product UI, terminal-controlled Unicode monospace, no market-specific assumptions. Regular desktop terminal use with compact information and keyboard control. Preserve the established cyan accent and quiet borders. Avoid decorative counters, animated dashboards and fabricated percent-complete values.

Token ownership: the existing Rust/Ratatui theme remains canonical, centralized as INK, MUTED, ACCENT, SURFACE, WARN and ERROR in `src/tui.rs`. This document mirrors those exact values. No browser CSS, DOM or font loading is involved. The setup wizard preserves its familiar terminal palette and preview behavior.

## Colors
Use the six frontmatter tokens. Cyan indicates focus/selection; amber attention; red failure. Every state also has an explicit text label. Respect `NO_COLOR` by using plain terminal colors where available; no status relies on color alone.

## Typography
All roles use the user's terminal font. Bold labels establish hierarchy, normal text carries content, and muted text carries supporting hints. Font family and size are controlled by the terminal, not the application. User Unicode input is retained.

## Layout
Fixed header, tab strip, one scrollable content area and four-row shortcut/status footer. At 100 columns show three task columns, at 68 show two, below that show one. Left/right traverses every board column. Below 42×12 show a resize instruction while preserving safe close behavior. Lists own their scroll; details use up/down. Footer actions remain reachable on every screen.

## Elevation & Depth
Square borders and a clear modal surface; no shadows or animation. Modal input owns keyboard focus and Esc returns to its source.

## Shapes
One-cell borders, no decorative icons or rounded corners. Selection has a visible text arrow.

## Components
### Foundational visual states
Ready, Missing, Failed and Not checked are distinct words. Busy operations retain navigable screens; repeated execution controls are disabled. Cached content remains visible with any read/sync error.
### Buttons and actions
Text shortcuts use consistent verbs. Start/Run explain configured external synchronization. Stop explains cancellation and unknown termination. Closing waits for the bounded operation, then pauses scheduling. Setup previews exact local changes before Y applies.
### Navigation and data display
Six screens: Home, Workflow, Tasks, Review, History, Settings. Task selection and detail links reference real saved identities. Empty lists explain the next useful action.
### Forms and overlays
Typed terminal fields with labels, Tab/Shift+Tab movement, Ctrl+U clear, Backspace edit, Enter commit and Esc cancel. Search is local and explicit Enter applies; C clears. Notion tokens are never input/display fields.
### Iconography
Text arrows and simple separators only; every operation has a written label.
### Motion
None beyond real status updates. No spinner or fake progress meter.
### Content and data visualization
Plain operational English. Durations display minutes/seconds or elapsed age; underlying history retains exact timestamps. Budgets distinguish agent calls from cycles and checks.

## Do's and Don'ts
- Do show the exact reason an action cannot proceed and a recovery step.
- Do retain existing workflow/approval rules through shared services.
- Don't equate Done with merged/released, or configured with verified.
- Don't conceal stale Notion data or turn card movement into authority.
