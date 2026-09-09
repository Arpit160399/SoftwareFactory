use serde_json::Value;
use softwarefactory::{
    console::{self, Action, Event},
    kanban,
    setup::*,
    tui,
    workflow::{self, WorkflowState},
};
use std::{
    fs,
    time::{Duration, Instant},
};
fn project(id: &str) -> (tempfile::TempDir, Profile) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".softwarefactory-synthetic-fixture"),
        "test only",
    )
    .unwrap();
    fs::write(dir.path().join("AGENTS.md"), "Preserve other files.\n").unwrap();
    let bridge = format!("{}/examples/fixture_bridge.py", env!("CARGO_MANIFEST_DIR"));
    let command = CommandSpec {
        program: bridge,
        args: vec![],
    };
    let mut p = Profile::template("generic", id);
    p.product_brief = "Produce an observable fixture result".into();
    p.runtime.command = command.clone();
    p.runtime.planner_model = "fixture-planner".into();
    p.review.command = Some(command.clone());
    p.review.reviewers = vec!["fixture-human".into()];
    p.checks = vec![Check {
        id: "result".into(),
        category: "delivered_behaviour".into(),
        required: true,
        command,
        criterion_ids: vec!["C1".into()],
        timeout_seconds: 10,
        resource: None,
    }];
    apply(&preview(dir.path(), &p).unwrap()).unwrap();
    (dir, p)
}

#[test]
fn read_only_home_reports_missing_capabilities_without_dispatch() {
    let (d, _) = project("console");
    let state = console::Snapshot::read(d.path());
    assert_eq!(state.name, "console");
    assert!(state.problem.is_none());
    assert!(state.requirements.iter().any(|r| r.status == "Not checked"));
    assert!(!d.path().join(".product-workflow/fixture-jobs").exists());
    console::probe(d.path()).unwrap();
    let state = console::Snapshot::read(d.path());
    assert!(
        state
            .requirements
            .iter()
            .any(|r| r.name == "Agent runtime and review source" && r.status == "Ready")
    );
}
#[test]
fn empty_and_narrow_screens_render_without_losing_navigation() {
    let (d, _) = project("console");
    for (width, height) in [(120, 36), (70, 22), (44, 14), (30, 8)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let mut app = tui::App::default();
        app.snapshot = console::Snapshot::read(d.path());
        for tab in 0..6 {
            app.tab = tab;
            terminal.draw(|f| tui::draw(f, &app, d.path())).unwrap();
            let buffer = terminal.backend().buffer();
            let text = buffer
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("Software Factory") || text.contains("SOFTWARE FACTORY"));
        }
    }
}
#[test]
fn projection_keeps_discovery_without_feature_and_human_gate_visible() {
    let (d, _) = project("cards");
    fs::write(d.path().join(".fixture-propose"), "yes").unwrap();
    let w = workflow::start(d.path(), "Improve onboarding", Some(1)).unwrap();
    for _ in 0..7 {
        let state = workflow::advance(d.path(), &w.id).unwrap();
        if state.state == WorkflowState::AwaitingBuildApproval {
            break;
        }
    }
    let cards = kanban::cards(d.path()).unwrap();
    assert_eq!(cards.len(), 2);
    assert!(
        cards
            .iter()
            .any(|c| c.kind == "Discovery" && c.status == "Done")
    );
    assert!(
        cards
            .iter()
            .any(|c| c.kind == "Feature" && c.status == "Needs approval" && c.agent == "none")
    );
}
fn board_project() -> tempfile::TempDir {
    let (d, mut p) = project("board");
    p.kanban = Some(kanban::Config {
        enabled: true,
        database_id: uuid::Uuid::new_v4().to_string(),
        data_source_id: uuid::Uuid::new_v4().to_string(),
        properties: kanban::default_properties(),
    });
    apply(&preview(d.path(), &p).unwrap()).unwrap();
    d
}
#[test]
fn sync_replays_same_card_after_lost_create_response() {
    let d = board_project();
    fs::write(d.path().join(".fixture-sync-uncertain"), "yes").unwrap();
    workflow::start(d.path(), "Discover", None).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_softwarefactory"))
        .arg("--project")
        .arg(d.path())
        .args(["board", "--sync"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    let failed = kanban::state(d.path()).unwrap();
    assert!(failed.error.is_some());
    assert_eq!(failed.pending(), 1);
    let dir = d.path().join(".product-workflow/kanban");
    let path = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .unwrap();
    let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    state["retry_at"] = 0.into();
    for entry in state["entries"].as_object_mut().unwrap().values_mut() {
        entry["retry_at"] = 0.into();
    }
    fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
    let done = kanban::sync(d.path()).unwrap();
    assert_eq!(done.pending(), 0);
    assert!(done.error.is_none());
    let files = fs::read_dir(d.path().join(".product-workflow/fixture-board"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|s| s == "json")
        })
        .count();
    assert_eq!(files, 1);
    let repeated = kanban::sync(d.path()).unwrap();
    assert_eq!(done.last_success, repeated.last_success);
    assert_eq!(repeated.entries.len(), 1);
}
#[test]
fn manual_configuration_cannot_redirect_board_writes() {
    let d = board_project();
    let path = d.path().join(".product-workflow/profile.json");
    let mut p: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    p["kanban"]["database_id"] = uuid::Uuid::new_v4().to_string().into();
    fs::write(path, serde_json::to_vec(&p).unwrap()).unwrap();
    assert!(kanban::sync(d.path()).is_err());
}
#[test]
fn worker_pauses_before_next_role_and_shutdown_preserves_progress() {
    let (d, mut p) = project("pause");
    let script = d.path().join("slow_bridge.py");
    let fixture = format!("{}/examples/fixture_bridge.py", env!("CARGO_MANIFEST_DIR"));
    fs::write(&script,format!("import json,sys,time,pathlib,subprocess\nr=json.load(sys.stdin)\nif r.get('role')=='research':\n pathlib.Path('.product-workflow/research-started').write_text('yes')\n time.sleep(0.8)\np=subprocess.run([{}],input=json.dumps(r),text=True,capture_output=True)\nprint(p.stdout)\nsys.exit(p.returncode)\n",serde_json::to_string(&fixture).unwrap())).unwrap();
    p.runtime.command = CommandSpec {
        program: "python3".into(),
        args: vec![script.to_string_lossy().into_owned()],
    };
    apply(&preview(d.path(), &p).unwrap()).unwrap();
    let worker = console::worker(d.path().to_path_buf());
    worker
        .tx
        .send(Action::Start("Discover".into(), None))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    while !d.path().join(".product-workflow/research-started").exists() {
        assert!(Instant::now() < deadline, "research did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
    worker.tx.send(Action::Pause).unwrap();
    worker.tx.send(Action::Shutdown).unwrap();
    let mut closed = false;
    while Instant::now() < deadline {
        if matches!(
            worker.rx.recv_timeout(Duration::from_millis(200)),
            Ok(Event::Closed)
        ) {
            closed = true;
            break;
        }
    }
    assert!(closed);
    let w = workflow::list(d.path()).unwrap();
    let discovery =
        softwarefactory::adapters::discovery_load(d.path(), &w[0].current_cycle().discovery_id)
            .unwrap();
    assert_eq!(discovery["next_stage"], 1);
    assert_eq!(discovery["outputs"].as_array().unwrap().len(), 1);
}

#[test]
fn task_selection_follows_identity_and_failed_refresh_keeps_known_context() {
    let (d, _) = project("selection");
    let w = workflow::start(d.path(), "Discover", None).unwrap();
    let snapshot = console::Snapshot::read(d.path());
    let original = snapshot.cards[0].record_key.clone();
    let mut app = tui::App::default();
    app.tab = 2;
    app.snapshot = snapshot.clone();
    app.detail = true;
    let mut inserted = snapshot.clone();
    let mut other = inserted.cards[0].clone();
    other.record_key = "other".into();
    inserted.cards.insert(0, other);
    app.apply_snapshot(inserted);
    assert_eq!(app.selected, 1);
    assert!(app.detail);
    let failed = console::Snapshot {
        problem: Some("Read unavailable".into()),
        ..Default::default()
    };
    app.apply_snapshot(failed);
    assert_eq!(app.snapshot.cards[1].record_key, original);
    assert_eq!(app.snapshot.workflows[0].id, w.id);
    assert!(app.notice.contains("last known"));
}

#[test]
fn one_failed_card_does_not_starve_unrelated_work() {
    let d = board_project();
    fs::write(d.path().join(".fixture-sync-uncertain"), "yes").unwrap();
    let first = workflow::start(d.path(), "First discovery", Some(1)).unwrap();
    for _ in 0..12 {
        if workflow::advance(d.path(), &first.id)
            .unwrap()
            .is_terminal()
        {
            break;
        }
    }
    workflow::start(d.path(), "Second discovery", None).unwrap();
    let state = kanban::sync(d.path()).unwrap();
    assert_eq!(state.pending(), 1);
    assert!(state.error.is_some());
    assert!(state.entries.values().any(|entry| entry.sent_sequence > 0));
}

#[test]
fn evidence_preview_is_bounded_and_cannot_read_another_run_or_source_file() {
    let (d, _) = project("evidence");
    let id = uuid::Uuid::new_v4().to_string();
    let directory = softwarefactory::adapters::run_dir(d.path(), &id).unwrap();
    fs::create_dir_all(&directory).unwrap();
    let file = directory.join("diff.txt");
    fs::write(&file, format!("\x1b[31m{}", "change\n".repeat(3000))).unwrap();
    let preview = tui::evidence_excerpt(d.path(), &id, file.to_str().unwrap()).unwrap();
    assert!(!preview.contains('\x1b'));
    assert!(preview.contains("limited to 12 KB"));
    assert!(tui::evidence_excerpt(d.path(), &id, "AGENTS.md").is_err());
    assert!(
        tui::evidence_excerpt(
            d.path(),
            &uuid::Uuid::new_v4().to_string(),
            file.to_str().unwrap()
        )
        .is_err()
    );
}
