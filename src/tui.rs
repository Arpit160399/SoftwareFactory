//! Keyboard control panel; all adapter calls execute on a separate worker.
use crate::{
    console::{self, Action, Event, Snapshot},
    kanban, setup, workflow,
};
use anyhow::{Result, ensure};
use crossterm::{
    event::{self, Event as TerminalEvent, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};
use std::{
    io::{self, IsTerminal},
    path::Path,
    time::Duration,
};
const INK: Color = Color::Rgb(220, 229, 237);
const MUTED: Color = Color::Rgb(157, 174, 190);
const ACCENT: Color = Color::Rgb(107, 202, 215);
const SURFACE: Color = Color::Rgb(23, 35, 49);
const WARN: Color = Color::Rgb(245, 196, 110);
const ERROR: Color = Color::Rgb(242, 145, 145);
const TABS: [&str; 6] = ["Home", "Workflow", "Tasks", "Review", "History", "Settings"];
const COLUMNS: [&str; 8] = [
    "Discovery",
    "Needs approval",
    "In progress",
    "Ready for review",
    "Blocked",
    "Done",
    "Deferred",
    "Cancelled",
];
#[derive(Default)]
pub struct App {
    pub snapshot: Snapshot,
    pub tab: usize,
    pub selected: usize,
    pub column: usize,
    pub scroll: u16,
    pub detail: bool,
    pub notice: String,
    pub running: Option<String>,
    pub busy: bool,
    pub closing: bool,
    pub filter: String,
    pub workflow_id: Option<String>,
    modal: Option<Modal>,
}
enum Modal {
    Start {
        question: String,
        limit: String,
        field: usize,
    },
    Confirm {
        label: String,
        action: Action,
    },
    Stop(String),
    Search(String),
    Text(String),
}
impl App {
    pub fn apply_snapshot(&mut self, mut snapshot: Snapshot) {
        if snapshot
            .observed_at
            .zip(self.snapshot.observed_at)
            .is_some_and(|(new, old)| new < old)
        {
            return;
        }
        if snapshot.problem.is_some() {
            if snapshot.workflows.is_empty() {
                snapshot.workflows = self.snapshot.workflows.clone();
            }
            if snapshot.cards.is_empty() {
                snapshot.cards = self.snapshot.cards.clone();
            }
            self.notice = format!(
                "Refresh failed; showing last known records: {}",
                snapshot.problem.as_deref().unwrap_or_default()
            );
        }
        let card_key = self.card().map(|card| card.record_key.clone());
        let workflow_key = self.workflow().map(|w| w.id.clone());
        self.snapshot = snapshot;
        if matches!(self.tab, 2 | 3) {
            if let Some(key) = card_key {
                if let Some(index) = self
                    .indices()
                    .iter()
                    .position(|i| self.snapshot.cards[*i].record_key == key)
                {
                    self.selected = index;
                } else {
                    self.detail = false;
                    self.selected = 0;
                    self.notice="Selected task moved out of this view; choose its new column or clear the filter.".into();
                }
            }
        } else if self.tab == 4
            && let Some(key) = workflow_key
            && let Some(index) = self.snapshot.workflows.iter().position(|w| w.id == key)
        {
            self.selected = index;
        }
        self.clamp();
    }
    fn workflow(&self) -> Option<&workflow::Workflow> {
        if self.tab == 4 {
            self.snapshot.workflows.get(self.selected)
        } else {
            self.running
                .as_ref()
                .or(self.workflow_id.as_ref())
                .and_then(|id| self.snapshot.workflows.iter().find(|w| &w.id == id))
                .or_else(|| {
                    self.snapshot
                        .workflows
                        .iter()
                        .rev()
                        .find(|w| !w.is_terminal())
                })
                .or_else(|| self.snapshot.workflows.last())
        }
    }
    fn indices(&self) -> Vec<usize> {
        self.snapshot
            .cards
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                (self.tab != 2 || c.status == COLUMNS[self.column])
                    && (self.tab != 3
                        || matches!(c.status.as_str(), "Needs approval" | "Ready for review"))
                    && (self.filter.is_empty()
                        || format!("{} {} {}", c.name, c.summary, c.stage)
                            .to_lowercase()
                            .contains(&self.filter.to_lowercase()))
            })
            .map(|(i, _)| i)
            .collect()
    }
    fn card(&self) -> Option<&kanban::Card> {
        self.indices()
            .get(self.selected)
            .and_then(|i| self.snapshot.cards.get(*i))
    }
    fn selection_len(&self) -> usize {
        if self.tab == 2 || self.tab == 3 {
            self.indices().len()
        } else {
            self.snapshot.workflows.len()
        }
    }
    fn clamp(&mut self) {
        self.selected = self.selected.min(self.selection_len().saturating_sub(1));
    }
}
fn panel(title: &str) -> Block<'_> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MUTED))
}
fn body(f: &mut Frame, area: Rect, title: &str, text: String, scroll: u16) {
    f.render_widget(
        Paragraph::new(safe_text(&text))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(panel(title)),
        area,
    );
}
fn list(f: &mut Frame, area: Rect, title: &str, items: Vec<String>, selected: usize) {
    let mut state =
        ListState::default().with_selected(if items.is_empty() || selected == usize::MAX {
            None
        } else {
            Some(selected.min(items.len() - 1))
        });
    f.render_stateful_widget(
        List::new(
            items
                .into_iter()
                .map(|item| ListItem::new(safe_text(&item)))
                .collect::<Vec<_>>(),
        )
        .block(panel(title))
        .highlight_symbol("› ")
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        area,
        &mut state,
    );
}
fn workflow_text(w: &workflow::Workflow) -> String {
    let cycle = w.current_cycle();
    let elapsed = setup::timestamp().saturating_sub(w.created_at);
    let remaining = w.max_seconds.saturating_sub(elapsed);
    let mut text = format!(
        "{}\n\nDiscover → Decide → Build → Verify → Learn → Repeat\n\nCycle {}  |  {:?}\nCalls used: {} / {}   |   Time left: {}m {}s\nWorkflow: {}\n\n",
        w.question,
        cycle.number,
        w.state,
        w.dispatches_used,
        w.max_dispatches,
        remaining / 60,
        remaining % 60,
        w.id
    );
    if let Some(reason) = &w.blocked_reason {
        text.push_str(&format!("Needs attention: {reason}\n\n"));
    }
    if w.is_waiting() {
        text.push_str("The runner is waiting for a verified decision or new direction.\n\n");
    }
    text.push_str("Recent activity\n");
    for e in w.events.iter().rev().take(20) {
        text.push_str(&format!(
            "• {}\n",
            e["message"].as_str().unwrap_or("Event recorded")
        ));
    }
    text
}
fn safe_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}
pub fn evidence_excerpt(root: &Path, run_id: &str, reference: &str) -> Result<String> {
    use std::io::Read;
    let root = std::fs::canonicalize(root)?;
    let candidate = if Path::new(reference).is_absolute() {
        Path::new(reference).to_path_buf()
    } else {
        root.join(reference)
    };
    let relative = candidate
        .strip_prefix(&root)
        .map_err(|_| anyhow::anyhow!("Evidence must remain in this project"))?;
    let path = setup::safe_path(
        &root,
        relative
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid evidence path"))?,
    )?;
    ensure!(
        path.starts_with(crate::adapters::run_dir(&root, run_id)?),
        "Evidence must belong to the selected feature run"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(12001)
        .read_to_end(&mut bytes)?;
    let clipped = bytes.len() > 12000;
    bytes.truncate(12000);
    Ok(format!(
        "{}{}",
        safe_text(&String::from_utf8_lossy(&bytes)),
        if clipped {
            "\n[Preview limited to 12 KB; see the referenced artifact for the full result]"
        } else {
            ""
        }
    ))
}
fn card_text(root: &Path, card: &kanban::Card) -> String {
    let mut text = format!(
        "{}\n{} · Cycle {} · Iteration {}\n\nStatus: {}\nStage: {}\nCurrent agent: {}\n\n{}\n\nBlocker: {}\nNext action: {}\n\nRevision: {}\nReview: {}\n",
        card.name,
        card.kind,
        card.cycle,
        card.iteration,
        card.status,
        card.stage,
        card.agent,
        card.summary,
        if card.blocker.is_empty() {
            "None recorded"
        } else {
            &card.blocker
        },
        if card.next_action.is_empty() {
            "No human action currently required"
        } else {
            &card.next_action
        },
        card.revision,
        if card.review_url.is_empty() {
            "No published review link yet"
        } else {
            &card.review_url
        }
    );
    if card.kind == "Feature"
        && let Some(id) = card.record_key.rsplit(':').next()
        && let Ok(run) = crate::adapters::load_run(root, id)
    {
        text.push_str("\nAcceptance criteria\n");
        for criterion in &run.proposal.criteria {
            text.push_str(&format!("• {}: {}\n", criterion.id, criterion.description));
        }
        if let Some(implementation) = &run.current().implementation {
            text.push_str(&format!(
                "\nChange summary\n{}\nKnown gaps: {}\n",
                implementation.summary,
                implementation.known_gaps.join("; ")
            ));
            for (label, reference) in [
                ("Code changes", &implementation.diff_ref),
                ("Guidance changes", &implementation.guidance_diff_ref),
            ] {
                let preview = evidence_excerpt(root, id, reference)
                    .unwrap_or_else(|error| format!("Evidence unavailable: {error}"));
                text.push_str(&format!("\n{label}\n{preview}\n"));
            }
        }
        if let Some(review) = &run.current().review {
            text.push_str(&format!("\nIndependent review\n{}\n", review.summary));
            for finding in &review.findings {
                text.push_str(&format!(
                    "{} · {}\nExpected: {}\nObserved: {}\nCause: {}\n",
                    finding.criterion_id,
                    finding.severity,
                    finding.expected,
                    finding.observed,
                    finding.cause
                ));
            }
        }
        text.push_str("\nChecks and evidence\n");
        for iteration in &run.iterations {
            for check in &iteration.checks {
                text.push_str(&format!(
                    "Iteration {} · {} · {:?} / {:?}\n",
                    iteration.number, check.id, check.execution, check.outcome
                ));
                for file in &check.evidence_refs {
                    text.push_str(&format!("  {file}\n"));
                }
                if let Some(error) = &check.error {
                    text.push_str(&format!("  {error}\n"));
                }
            }
        }
        text.push_str("\nHuman decisions (reverified by the coordinator before use)\n");
        for d in &run.decisions {
            text.push_str(&format!(
                "{:?} · {} · {}\n",
                d.action, d.actor, d.decided_at
            ));
        }
    }
    if card.kind == "Harness improvement"
        && let Ok(w) = workflow::load(root, &card.workflow_id)
        && let Some(cycle) = w.cycles.iter().find(|cycle| cycle.number == card.cycle)
        && let Some(id) = &cycle.learning_id
    {
        match crate::learning::read(root, id) {
            Ok(record) => {
                let input = &record["input"];
                text.push_str(&format!("\nHarness comparison\nDiagnosis: {}\nBaseline: {}\nCandidate: {}\nGains: {}\nRegressions: {}\nGaps: {}\n",input["diagnosis"].as_str().unwrap_or(""),input["baseline"]["version"],input["candidate"]["version"],input["gains"],input["regressions"],input["gaps"]));
                if let Some(cases) = input["comparisons"].as_array() {
                    for case in cases {
                        text.push_str(&format!(
                            "{} · {}\nBaseline evidence: {}\nCandidate evidence: {}\n",
                            case["scenario_id"],
                            case["split"],
                            case["baseline_evidence_ref"],
                            case["candidate_evidence_ref"]
                        ));
                    }
                }
            }
            Err(error) => text.push_str(&format!("\nComparison unavailable: {error}")),
        }
    }
    text
}
pub fn draw(f: &mut Frame, app: &App, root: &Path) {
    f.render_widget(
        Block::default().style(Style::default().fg(INK).bg(SURFACE)),
        f.area(),
    );
    if f.area().width < 42 || f.area().height < 12 {
        body(f,f.area(),"Software Factory","Terminal too small. Resize to at least 42 columns × 12 rows.\nQ closes safely; active work finishes its current stage.".into(),0);
        return;
    }
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(4),
        ])
        .split(f.area());
    let status = if app.closing {
        "Closing after current operation"
    } else if app.busy {
        "Working · controls remain available"
    } else if app.running.is_some() {
        "Runner active"
    } else {
        "Runner paused"
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    " SOFTWARE FACTORY ",
                    Style::default()
                        .fg(SURFACE)
                        .bg(ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("  {}", safe_text(&app.snapshot.name))),
            ]),
            Line::from(format!(" {status}")),
        ]),
        areas[0],
    );
    f.render_widget(
        Tabs::new(TABS.to_vec())
            .select(app.tab)
            .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
            .divider("│"),
        areas[1],
    );
    let main = areas[2];
    match app.tab {
        0 => {
            let mut text = format!("{}\n\n", app.snapshot.brief);
            if let Some(problem) = &app.snapshot.problem {
                text.push_str(&format!("Needs attention: {problem}\n\n"));
            }
            if app.snapshot.workflows.is_empty() {
                text.push_str("No workflow yet.\nE  Configure this project\nP  Check prerequisites\nN  Start with a product objective\n\n");
            } else if let Some(w) = app
                .snapshot
                .workflows
                .iter()
                .rev()
                .find(|w| !w.is_terminal())
                .or_else(|| app.snapshot.workflows.last())
            {
                text.push_str(&workflow_text(w));
            }
            text.push_str("\nPrerequisites\n");
            for r in &app.snapshot.requirements {
                text.push_str(&format!("[{}] {} — {}\n", r.status, r.name, r.detail));
            }
            text.push_str("\nNotion sync\n");
            if let Some(s) = &app.snapshot.sync {
                text.push_str(&format!(
                    "{} queued · Last success: {}\n{}",
                    s.pending(),
                    s.last_success
                        .map(|at| format!("{}s ago", setup::timestamp().saturating_sub(at)))
                        .unwrap_or("Never synchronized".into()),
                    s.error
                        .as_deref()
                        .unwrap_or("No synchronization error recorded")
                ));
            } else {
                text.push_str("Not configured. Local progress remains available.\n");
            }
            body(f, main, "Project home", text, app.scroll);
        }
        1 | 4 => {
            if app.snapshot.workflows.is_empty() {
                body(
                    f,
                    main,
                    "Workflow history",
                    "No saved workflows. Press N to start one after setup is ready.".into(),
                    0,
                );
            } else if app.detail || app.tab == 1 {
                if let Some(w) = app.workflow() {
                    body(
                        f,
                        main,
                        if app.tab == 1 {
                            "Workflow dashboard"
                        } else {
                            "Cycle history"
                        },
                        if app.tab == 4 {
                            format!(
                                "{}\n\n{}",
                                workflow_text(w),
                                w.cycles
                                    .iter()
                                    .map(|c| format!(
                                        "Cycle {}: {}\nLearning: {}\n",
                                        c.number,
                                        c.question,
                                        c.retrospective_summary
                                            .as_deref()
                                            .unwrap_or("Not recorded yet")
                                    ))
                                    .collect::<String>()
                            )
                        } else {
                            workflow_text(w)
                        },
                        app.scroll,
                    );
                }
            } else {
                list(
                    f,
                    main,
                    "Saved workflows · Enter opens cycles",
                    app.snapshot
                        .workflows
                        .iter()
                        .map(|w| {
                            format!(
                                "{:?} · Cycle {}\n{}",
                                w.state,
                                w.current_cycle().number,
                                w.question
                            )
                        })
                        .collect(),
                    app.selected,
                );
            }
        }
        2 | 3 => {
            if app.detail {
                body(
                    f,
                    main,
                    "Task details · Esc returns · O opens review",
                    app.card()
                        .map(|c| card_text(root, c))
                        .unwrap_or("This task is no longer in the current filter.".into()),
                    app.scroll,
                );
            } else if app.tab == 3 {
                let items = app
                    .indices()
                    .iter()
                    .map(|i| {
                        let c = &app.snapshot.cards[*i];
                        format!("{} · {}\n{}", c.status, c.name, c.next_action)
                    })
                    .collect::<Vec<_>>();
                if items.is_empty() {
                    body(f,main,"Review inbox","No decisions are waiting. New requests appear here with the exact proposal or result to review.".into(),0);
                } else {
                    list(f, main, "Review inbox · Enter details", items, app.selected);
                }
            } else {
                let count = if main.width >= 100 {
                    3
                } else if main.width >= 68 {
                    2
                } else {
                    1
                };
                let start = app.column.saturating_sub(count - 1);
                let parts = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(vec![Constraint::Ratio(1, count as u32); count])
                    .split(main);
                for (offset, area) in parts.iter().enumerate() {
                    let col = start + offset;
                    let items = app
                        .snapshot
                        .cards
                        .iter()
                        .filter(|c| {
                            c.status == COLUMNS[col]
                                && (app.filter.is_empty()
                                    || format!("{} {} {}", c.name, c.summary, c.stage)
                                        .to_lowercase()
                                        .contains(&app.filter.to_lowercase()))
                        })
                        .map(|c| format!("{}\n{} · Cycle {}\n{}", c.name, c.kind, c.cycle, c.stage))
                        .collect::<Vec<_>>();
                    if items.is_empty() {
                        body(f, *area, COLUMNS[col], "No tasks in this column.".into(), 0);
                    } else {
                        list(
                            f,
                            *area,
                            &format!(
                                "{}{}",
                                if col == app.column { "› " } else { "" },
                                COLUMNS[col]
                            ),
                            items,
                            if col == app.column {
                                app.selected
                            } else {
                                usize::MAX
                            },
                        );
                    }
                }
            }
        }
        _ => body(
            f,
            main,
            "Project settings",
            app.snapshot.settings.clone(),
            app.scroll,
        ),
    }
    let actions = if app.detail {
        "↑↓ Scroll · Esc Back · O Open review"
    } else if app.tab == 2 {
        "←→ Columns · ↑↓ Tasks · Enter Details · / Search · C Clear"
    } else {
        "N New · R Run · B Resume · Space Pause · X Stop · E Setup"
    };
    let notice = if app.closing {
        "Waiting for the bounded operation to finish. No next stage will start."
    } else {
        &app.notice
    };
    f.render_widget(
        Paragraph::new(format!(
            "Tab Screens · {actions}\nP Check prerequisites · S Sync board · Q Close\n{notice}"
        ))
        .style(Style::default().fg(
            if app.notice.contains("failed") || app.notice.contains("unavailable") {
                ERROR
            } else {
                WARN
            },
        ))
        .wrap(Wrap { trim: false }),
        areas[3],
    );
    if let Some(modal) = &app.modal {
        let w = f.area().width.saturating_sub(6).min(86);
        let h = f.area().height.saturating_sub(4).min(18);
        let rect = Rect::new((f.area().width - w) / 2, (f.area().height - h) / 2, w, h);
        f.render_widget(Clear, rect);
        let text = match modal {
            Modal::Start {
                question,
                limit,
                field,
            } => format!(
                "Start a whole workflow\n\n{} Product objective:\n{}\n\n{} Cycle limit (blank keeps cycling within budgets): {}\n\nTab changes field. Enter starts. Esc cancels.\nConfigured review packets and enabled board sync may be written. Human approvals remain required.",
                if *field == 0 { "›" } else { " " },
                question,
                if *field == 1 { "›" } else { " " },
                limit
            ),
            Modal::Confirm { label, .. } => format!("{label}\n\nY Continue · Esc Cancel"),
            Modal::Stop(id) => format!(
                "Stop workflow {id}?\n\nRequests cancellation of active work and stops this loop.\nUnknown termination stays blocked until reconciled.\n\nY Stop workflow · Esc Keep running"
            ),
            Modal::Search(query) => {
                format!("Filter tasks\n\n{query}▏\n\nEnter Apply · Ctrl+U Clear · Esc Cancel")
            }
            Modal::Text(text) => format!("{text}\n\nEsc closes"),
        };
        body(f, rect, "Action", text, app.scroll);
    }
    if std::env::var_os("NO_COLOR").is_some() {
        for cell in &mut f.buffer_mut().content {
            cell.set_fg(Color::Reset).set_bg(Color::Reset);
        }
    }
}
struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}
fn open_review(url: &str) -> Result<()> {
    ensure!(
        url.starts_with("https://www.notion.so/") || url.starts_with("https://notion.so/"),
        "No trusted Notion review URL is available"
    );
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let status = std::process::Command::new(program).arg(url).status()?;
    ensure!(status.success(), "Could not open the review link");
    Ok(())
}
pub fn run(root: &Path) -> Result<()> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "TUI requires an interactive terminal"
    );
    let mut root = std::fs::canonicalize(root)?;
    let mut return_setup = false;
    loop {
        if return_setup {
            root = crate::setup_tui::run(&root)?;
        }
        enable_raw_mode()?;
        let restore = Restore;
        execute!(io::stdout(), EnterAlternateScreen)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        let worker = console::worker(root.clone());
        let mut app = App {
            notice: "Local status loads automatically. P verifies configured integrations.".into(),
            ..Default::default()
        };
        let mut shutdown_for_setup = false;
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<String>();
        let mut stopping = false;
        loop {
            if let Ok(message) = stop_rx.try_recv() {
                app.notice = message;
                stopping = false;
            }
            while let Ok(event) = worker.rx.try_recv() {
                match event {
                    Event::Snapshot(s) => {
                        app.apply_snapshot(*s);
                    }
                    Event::Message(message) => {
                        if app.tab == 5 && !app.closing {
                            app.modal = Some(Modal::Text(message.clone()));
                        }
                        app.notice = message;
                    }
                    Event::Running(id) => app.running = id,
                    Event::Busy(value) => app.busy = value,
                    Event::Closed => {
                        break;
                    }
                }
            }
            if app.closing && worker.tx.send(Action::Refresh).is_err() && !stopping {
                return_setup = shutdown_for_setup;
                break;
            }
            terminal.draw(|f| draw(f, &app, &root))?;
            if !event::poll(Duration::from_millis(80))? {
                continue;
            }
            let TerminalEvent::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if app.closing {
                continue;
            }
            if let Some(modal) = app.modal.take() {
                match modal {
                    Modal::Start {
                        mut question,
                        mut limit,
                        mut field,
                    } => match key.code {
                        KeyCode::Esc => {}
                        KeyCode::Tab => {
                            field = 1 - field;
                            app.modal = Some(Modal::Start {
                                question,
                                limit,
                                field,
                            });
                        }
                        KeyCode::Enter => {
                            let cap = if limit.is_empty() {
                                Ok(None)
                            } else {
                                limit.parse::<u32>().map(Some)
                            };
                            if question.trim().is_empty()
                                || cap.as_ref().is_err()
                                || cap.as_ref().is_ok_and(|v| *v == Some(0))
                            {
                                app.notice="Supply an objective and a positive cycle limit, or leave the limit blank".into();
                                app.modal = Some(Modal::Start {
                                    question,
                                    limit,
                                    field,
                                });
                            } else {
                                let _ = worker.tx.send(Action::Start(question, cap.unwrap()));
                                app.busy = true;
                            }
                        }
                        _ => {
                            let input = if field == 0 {
                                &mut question
                            } else {
                                &mut limit
                            };
                            match key.code {
                                KeyCode::Char('u')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    input.clear()
                                }
                                KeyCode::Char(c) if !c.is_control() && input.len() < 4000 => {
                                    input.push(c)
                                }
                                KeyCode::Backspace => {
                                    input.pop();
                                }
                                _ => {}
                            }
                            app.modal = Some(Modal::Start {
                                question,
                                limit,
                                field,
                            });
                        }
                    },
                    Modal::Confirm { label, action } => match key.code {
                        KeyCode::Char('y' | 'Y') => {
                            let _ = worker.tx.send(action);
                            app.busy = true;
                        }
                        KeyCode::Esc => {}
                        _ => app.modal = Some(Modal::Confirm { label, action }),
                    },
                    Modal::Stop(id) => match key.code {
                        KeyCode::Char('y' | 'Y') => {
                            let root = root.clone();
                            let tx = worker.tx.clone();
                            let stop_tx = stop_tx.clone();
                            stopping = true;
                            std::thread::spawn(move || {
                                let result = workflow::stop(&root, &id, "Stopped from the TUI");
                                let message = match result {
                                    Ok(state) => format!(
                                        "Stop status: {:?}. {}",
                                        state.state,
                                        state.blocked_reason.unwrap_or_default()
                                    ),
                                    Err(error) => format!("Stop request needs attention: {error}"),
                                };
                                let _ = stop_tx.send(message);
                                let _ = tx.send(Action::Refresh);
                            });
                            app.notice="Stop requested. Cancellation must be confirmed before this workflow is stopped.".into();
                        }
                        KeyCode::Esc => {}
                        _ => app.modal = Some(Modal::Stop(id)),
                    },
                    Modal::Search(mut query) => match key.code {
                        KeyCode::Esc => {}
                        KeyCode::Enter => {
                            app.filter = query;
                            app.selected = 0;
                            app.detail = false;
                        }
                        _ => {
                            match key.code {
                                KeyCode::Char('u')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    query.clear()
                                }
                                KeyCode::Backspace => {
                                    query.pop();
                                }
                                KeyCode::Char(c) if !c.is_control() => query.push(c),
                                _ => {}
                            }
                            app.modal = Some(Modal::Search(query));
                        }
                    },
                    Modal::Text(text) => {
                        if key.code != KeyCode::Esc {
                            if key.code == KeyCode::Down {
                                app.scroll = app.scroll.saturating_add(1);
                            }
                            if key.code == KeyCode::Up {
                                app.scroll = app.scroll.saturating_sub(1);
                            }
                            app.modal = Some(Modal::Text(text));
                        }
                    }
                }
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('c')
                    if key.code == KeyCode::Char('q')
                        || key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    app.closing = true;
                    let _ = worker.tx.send(Action::Shutdown);
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    app.tab = if key.code == KeyCode::Tab {
                        (app.tab + 1) % 6
                    } else {
                        (app.tab + 5) % 6
                    };
                    app.detail = false;
                    app.selected = 0;
                    app.scroll = 0;
                }
                KeyCode::Char('1'..='6') => {
                    if let KeyCode::Char(c) = key.code {
                        app.tab = (c as u8 - b'1') as usize;
                        app.detail = false;
                        app.selected = 0;
                        app.scroll = 0;
                    }
                }
                KeyCode::Esc => {
                    app.detail = false;
                    app.scroll = 0;
                }
                KeyCode::Down => {
                    if app.detail || matches!(app.tab, 0 | 1 | 5) {
                        app.scroll = app.scroll.saturating_add(1);
                    } else {
                        app.selected =
                            (app.selected + 1).min(app.selection_len().saturating_sub(1));
                    }
                }
                KeyCode::Up => {
                    if app.detail || matches!(app.tab, 0 | 1 | 5) {
                        app.scroll = app.scroll.saturating_sub(1);
                    } else {
                        app.selected = app.selected.saturating_sub(1);
                    }
                }
                KeyCode::Left | KeyCode::Right if app.tab == 2 && !app.detail => {
                    app.column = if key.code == KeyCode::Right {
                        (app.column + 1) % 8
                    } else {
                        (app.column + 7) % 8
                    };
                    app.selected = 0;
                }
                KeyCode::Enter if matches!(app.tab, 2..=4) => {
                    if app.tab == 4 {
                        app.workflow_id = app.workflow().map(|w| w.id.clone());
                    }
                    app.detail = true;
                    app.scroll = 0;
                }
                KeyCode::Char('/') if app.tab == 2 || app.tab == 3 => {
                    app.modal = Some(Modal::Search(app.filter.clone()))
                }
                KeyCode::Char('c') => {
                    app.filter.clear();
                    app.selected = 0;
                    app.detail = false;
                }
                KeyCode::Char(' ') => {
                    let _ = worker.tx.send(Action::Pause);
                    app.notice="Pause requested. The current bounded stage will finish; no next stage starts.".into();
                }
                KeyCode::Char('n') if !app.busy && app.running.is_none() => {
                    app.modal = Some(Modal::Start {
                        question: String::new(),
                        limit: String::new(),
                        field: 0,
                    })
                }
                KeyCode::Char('r' | 'b') if !app.busy && app.running.is_none() => {
                    if let Some(w) = app.workflow() {
                        app.modal = Some(Modal::Confirm {
                            label: format!(
                                "{} workflow {}?\nConfigured review packets and enabled board sync may be written.",
                                if key.code == KeyCode::Char('b') {
                                    "Resume"
                                } else {
                                    "Run"
                                },
                                w.id
                            ),
                            action: if key.code == KeyCode::Char('b') {
                                Action::Resume(w.id.clone())
                            } else {
                                Action::Run(w.id.clone())
                            },
                        });
                    } else {
                        app.notice =
                            "Select a saved workflow in History, or press N to start.".into();
                    }
                }
                KeyCode::Char('x') => {
                    if let Some(id) = app
                        .running
                        .clone()
                        .or_else(|| app.workflow().map(|w| w.id.clone()))
                    {
                        app.modal = Some(Modal::Stop(id));
                    }
                }
                KeyCode::Char('e') if !app.busy && app.running.is_none() => {
                    shutdown_for_setup = true;
                    app.closing = true;
                    let _ = worker.tx.send(Action::Shutdown);
                }
                KeyCode::Char('p') if !app.busy => {
                    let _ = worker.tx.send(Action::Probe);
                    app.busy = true;
                    app.notice = "Checking configured capabilities and connections…".into();
                }
                KeyCode::Char('s') if !app.busy => {
                    app.modal=Some(Modal::Confirm{label:"Synchronize task cards to the configured Notion database? This updates external records.".into(),action:Action::Sync});
                }
                KeyCode::Char('o') => {
                    if let Some(c) = app.card() {
                        app.notice = open_review(&c.review_url)
                            .map(|_| "Opened review page".into())
                            .unwrap_or_else(|e| e.to_string());
                    }
                }
                KeyCode::Char('v') if app.tab == 5 && !app.busy => {
                    let _ = worker.tx.send(Action::Versions);
                    app.busy = true;
                }
                KeyCode::Char('u') if app.tab == 5 && !app.busy => {
                    let _ = worker.tx.send(Action::UpdateCheck);
                    app.busy = true;
                }
                KeyCode::Char('n' | 'r' | 'b' | 'e') => {
                    app.notice="Pause the runner and wait for its current operation before using this action.".into();
                }
                _ => {}
            }
        }
        drop(terminal);
        drop(restore);
        if !return_setup {
            break;
        }
    }
    Ok(())
}
