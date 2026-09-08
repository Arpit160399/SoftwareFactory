use crate::setup::{self, Profile, SetupPlan};
use anyhow::{Context, Result, ensure};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::Duration,
};

struct Screen {
    fields: Vec<(String, String)>,
    selected: usize,
    stage: u8,
    plan: Option<SetupPlan>,
    notice: String,
    scroll: u16,
}
impl Screen {
    fn new(root: &Path) -> Self {
        let existing = setup::load_profile(root).ok();
        let id = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my-project")
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        let p = existing.unwrap_or_else(|| Profile::template("generic", &id));
        Self {
            fields: vec![
                ("Project directory".into(), root.display().to_string()),
                ("Template (generic / meal-map)".into(), p.template),
                ("Stable project ID".into(), p.project_id),
                ("Product name".into(), p.name),
                ("Product brief".into(), p.product_brief),
                ("Runtime executable".into(), p.runtime.command.program),
                (
                    "Planner model (high reasoning)".into(),
                    p.runtime.planner_model,
                ),
                (
                    "Review bridge executable".into(),
                    p.review.command.map(|c| c.program).unwrap_or_default(),
                ),
                (
                    "Reviewer IDs (comma separated)".into(),
                    p.review.reviewers.join(","),
                ),
                ("Checks JSON file (optional)".into(), String::new()),
            ],
            selected: 0,
            stage: 0,
            plan: None,
            notice: String::new(),
            scroll: 0,
        }
    }
    fn profile(&self) -> Result<Profile> {
        let root = PathBuf::from(&self.fields[0].1);
        let mut p = setup::load_profile(&root)
            .unwrap_or_else(|_| Profile::template(&self.fields[1].1, &self.fields[2].1));
        p.template = self.fields[1].1.clone();
        p.project_id = self.fields[2].1.clone();
        p.name = self.fields[3].1.clone();
        p.product_brief = self.fields[4].1.clone();
        p.runtime.command.program = self.fields[5].1.clone();
        p.runtime.planner_model = self.fields[6].1.clone();
        p.review.command = if self.fields[7].1.is_empty() {
            None
        } else {
            Some(setup::CommandSpec {
                program: self.fields[7].1.clone(),
                args: p.review.command.map(|c| c.args).unwrap_or_default(),
            })
        };
        p.review.reviewers = self.fields[8]
            .1
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        if !self.fields[9].1.is_empty() {
            p.checks = serde_json::from_str(
                &std::fs::read_to_string(&self.fields[9].1)
                    .context("Cannot read check definitions")?,
            )?;
        }
        p.validate()?;
        Ok(p)
    }
    fn preview(&mut self) -> Result<()> {
        let p = self.profile()?;
        let root = Path::new(&self.fields[0].1);
        self.plan = Some(setup::preview(root, &p)?);
        let readiness = setup::readiness(root, &p);
        self.notice = format!(
            "{}\n{}",
            if readiness.ready {
                "Configured; verify capabilities before a proposal"
            } else {
                "Setup incomplete: required capabilities are missing"
            },
            setup::json(&readiness)?
        );
        self.stage = 1;
        self.scroll = 0;
        Ok(())
    }
}
struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}
pub fn run(root: &Path) -> Result<()> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "TUI requires an interactive terminal. Use setup --help for scripted setup."
    );
    enable_raw_mode()?;
    let _restore = Restore;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut app = Screen::new(root);
    loop {
        terminal.draw(|f|{let area=f.area();let parts=Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(4),Constraint::Min(5),Constraint::Length(3)]).split(area);
 let title=Paragraph::new(vec![Line::from(vec![Span::styled(" SOFTWARE FACTORY ",Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),Span::raw(format!("  {}  ·  PROJECT SETUP",setup::VERSION))]),Line::from(" Configure a reusable workflow. Your project stays in control.")]).block(Block::default().borders(Borders::BOTTOM));f.render_widget(title,parts[0]);
 match app.stage{0=>{let mut lines=vec![];for(i,(label,value))in app.fields.iter().enumerate(){let active=i==app.selected;lines.push(ListItem::new(vec![Line::from(Span::styled(format!("{} {}",if active{"›"}else{" "},label),Style::default().fg(if active{Color::Cyan}else{Color::Gray}))),Line::from(Span::styled(format!("  {}{}",if value.is_empty(){"(not configured)"}else{value},if active{" ▏"}else{""}),Style::default().fg(if active{Color::White}else{Color::DarkGray})))]));}let mut state=ratatui::widgets::ListState::default().with_selected(Some(app.selected));f.render_stateful_widget(List::new(lines).block(Block::default().title(" 1 · Select project and integrations ").borders(Borders::ALL)),parts[1],&mut state);},1=>{let plan=app.plan.as_ref().unwrap();let mut body=format!("Project: {}\nRelease: {}\n\n",plan.root.display(),setup::VERSION);for c in &plan.changes{body.push_str(&format!("{} {}\n",if c.before.is_some(){"MODIFY"}else{"CREATE"},c.path));if let Some(after)=&c.after{body.push_str(after);body.push('\n');}}if plan.changes.is_empty(){body.push_str("Already configured. No files will change.\n");}body.push_str("\nReadiness (no commands executed):\n");body.push_str(&app.notice);body.push_str("\nSample review packet (preview only):\nProject: selected project\nProposal revision: supplied per feature\nCriteria and journeys: supplied per feature\nBuild approval: pending\nAcceptance / merge / release: separate human decisions\n");body.push_str("\n\nPress Y to apply these exact configuration changes.\nExisting AGENTS.md and project source remain user-owned.\nSetup does not start a feature, execute checks or create external records.");f.render_widget(Paragraph::new(body).wrap(Wrap{trim:false}).scroll((app.scroll,0)).block(Block::default().title(" 2 · Review exact changes ").borders(Borders::ALL)),parts[1]);},_=>{f.render_widget(Paragraph::new(format!("Configuration saved.\n\n{}\n\nNext: configure any missing capabilities, then run doctor --probe.\nCreate and approve a feature proposal separately.\n\nPress Enter to close.",app.notice)).wrap(Wrap{trim:false}).scroll((app.scroll,0)).block(Block::default().title(" 3 · Setup result ").borders(Borders::ALL)),parts[1]);}}
 let hint=if app.stage==0{"Tab / Shift+Tab  Move   Enter  Review   Ctrl+U  Clear   Esc  Cancel"}else if app.stage==1{"↑ ↓  Scroll   Y  Apply exact changes   Esc  Back"}else{"↑ ↓  Scroll   Enter  Close"};f.render_widget(Paragraph::new(format!("{hint}\n{}",if app.stage==0{&app.notice}else{""})).style(Style::default().fg(Color::Cyan)),parts[2]);})?;
        if event::poll(Duration::from_millis(150))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                break;
            }
            match (app.stage, key.code) {
                (0, KeyCode::Esc) => break,
                (0, KeyCode::Tab) => app.selected = (app.selected + 1) % app.fields.len(),
                (0, KeyCode::BackTab) => {
                    app.selected = (app.selected + app.fields.len() - 1) % app.fields.len()
                }
                (0, KeyCode::Enter) => {
                    if let Err(e) = app.preview() {
                        app.notice = e.to_string();
                    }
                }
                (0, KeyCode::Char('u')) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.fields[app.selected].1.clear()
                }
                (0, KeyCode::Char(c)) => app.fields[app.selected].1.push(c),
                (0, KeyCode::Backspace) => {
                    app.fields[app.selected].1.pop();
                }
                (1, KeyCode::Esc) => app.stage = 0,
                (1, KeyCode::Char('y' | 'Y')) => match setup::apply(app.plan.as_ref().unwrap()) {
                    Ok(()) => {
                        app.stage = 2;
                        app.scroll = 0;
                    }
                    Err(e) => {
                        app.stage = 0;
                        app.notice = e.to_string();
                    }
                },
                (1 | 2, KeyCode::Down) => app.scroll = app.scroll.saturating_add(1),
                (1 | 2, KeyCode::Up) => app.scroll = app.scroll.saturating_sub(1),
                (2, KeyCode::Enter | KeyCode::Esc) => break,
                _ => {}
            }
        }
    }
    Ok(())
}
