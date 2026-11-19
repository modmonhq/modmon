use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use super::{
    render, render_guided_state, render_providers, render_secret_prompt_overlay,
    render_small_terminal, step_position,
};
use crate::domain::onboarding::{WorkflowState, WorkflowStep};
use crate::domain::secret::DirectSecretRotationPlan;
use crate::domain::workspace::ProjectId;
use crate::presentation::tui::fixtures::{cluster_model, remote_model, rich_model, setup_view};
use crate::presentation::tui::model::{LiveState, Model, Tab};
use crate::presentation::tui::provider::{
    ConnectionScope, ProviderModel, ProviderReview, ProviderScreen, SecretBackendTuiContext,
};

const SIZES: [(u16, u16); 4] = [(60, 20), (80, 24), (120, 40), (170, 48)];

fn draw(model: &Model, width: u16, height: u16) -> Result<String, Box<dyn std::error::Error>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| render(frame, model))?;
    Ok(buffer_text(terminal.backend().buffer()))
}

#[test]
fn every_view_renders_at_every_supported_size() -> Result<(), Box<dyn std::error::Error>> {
    let mut model = rich_model()?;
    for tab in Tab::ALL {
        model.tab = tab;
        for (width, height) in SIZES {
            let rendered = draw(&model, width, height)?;
            assert!(
                rendered.contains(" mm "),
                "{tab:?} {width}x{height} lost the header"
            );
            assert!(
                rendered.contains(tab.title()),
                "{tab:?} {width}x{height} lost its title"
            );
            assert!(
                rendered.contains("help"),
                "{tab:?} {width}x{height} lost the footer"
            );
        }
    }
    Ok(())
}

#[test]
fn views_show_live_status_semantically() -> Result<(), Box<dyn std::error::Error>> {
    let mut model = rich_model()?;
    for (tab, markers) in [
        (
            Tab::Overview,
            &["2/4", "migrate exited with code 137", "trusted"][..],
        ),
        (
            Tab::Services,
            &[
                "degraded",
                "stopped",
                "ingress",
                "control api",
                "fixture-web-1",
            ][..],
        ),
        (
            Tab::Ingress,
            &[
                "https://web.fixture.mm.test",
                "no HTTP port declared",
                "automatic convention",
            ][..],
        ),
        (
            Tab::Variables,
            &[
                "DATABASE_URL",
                "postgres://db/app",
                "••••••",
                "unset",
                "1 problem",
            ][..],
        ),
        (
            Tab::Diagnostics,
            &["runtime.container.exited", "Check memory limits"][..],
        ),
        (
            Tab::Tls,
            &[
                "generation 3",
                "user certificate store",
                "web.fixture.mm.test",
            ][..],
        ),
        (Tab::Jobs, &["cleanup", "nightly", "running"][..]),
    ] {
        model.tab = tab;
        let rendered = draw(&model, 170, 48)?;
        for marker in markers {
            assert!(rendered.contains(marker), "{tab:?} is missing {marker}");
        }
        assert!(
            !rendered.contains("hidden-value"),
            "{tab:?} leaked a sensitive value"
        );
    }
    Ok(())
}

#[test]
fn chrome_reports_health_navigation_and_refresh_failures() -> Result<(), Box<dyn std::error::Error>>
{
    let mut model = rich_model()?;
    let wide = draw(&model, 170, 48)?;
    for marker in [
        "2/4 running",
        "2 issues",
        "live",
        "WORKSPACE",
        "SECURITY",
        "7  Diagnostics",
        "2  Environments",
        " development ",
    ] {
        assert!(wide.contains(marker), "wide layout is missing {marker}");
    }
    let narrow = draw(&model, 80, 24)?;
    assert!(narrow.contains("1 Overview"));
    assert!(!narrow.contains("SECURITY"));
    assert!(narrow.contains("›"), "views that do not fit are hinted at");
    model.tab = Tab::Mounts;
    let scrolled = draw(&model, 80, 24)?;
    assert!(scrolled.contains("6 Mounts"));
    assert!(
        scrolled.contains("‹"),
        "the strip follows the selected view"
    );
    model.tab = Tab::Overview;

    model.live = LiveState::Failed("runtime socket closed".to_owned());
    let failed = draw(&model, 170, 48)?;
    assert!(failed.contains("refresh failed"));
    assert!(failed.contains("runtime socket closed"));

    model.help = true;
    let help = draw(&model, 120, 40)?;
    assert!(help.contains("open view by number"));
    assert!(help.contains("esc close"));
    Ok(())
}

#[test]
fn overview_lists_setup_steps_once_in_plain_words() -> Result<(), Box<dyn std::error::Error>> {
    use crate::domain::diagnostic::Diagnostic;
    use crate::presentation::tui::environments::EnvironmentEntry;

    let mut model = rich_model()?;
    model.setup = setup_view();
    model.environments.entries = ["development", "staging"]
        .map(|name| EnvironmentEntry {
            name: name.to_owned(),
            ..EnvironmentEntry::default()
        })
        .to_vec();
    // The same finding as the first step, as the runtime status reports it.
    model.snapshot.validation.push(Diagnostic::error(
        "MM_BUILD_REQUIRED",
        "services need reviewed image digests: api/api, docs/docs, web/web, web/queue",
    ));
    for (width, height) in SIZES {
        let rendered = draw(&model, width, height)?;
        for marker in [
            "Set up development",
            "2 steps left",
            "1  Build the images",
            "run step",
        ] {
            assert!(
                rendered.contains(marker),
                "{width}x{height} is missing {marker}:\n{rendered}"
            );
        }
        assert!(
            !rendered.contains("reviewed image digests"),
            "{width}x{height} repeats a step as a diagnostic"
        );
        assert!(!rendered.contains("The stack is not running"));
    }
    let wide = draw(&model, 140, 40)?;
    assert!(
        wide.contains("2 setup steps"),
        "the header counts the steps"
    );
    assert!(!wide.contains("no issues"));
    for marker in [
        "development cannot start until these are done.",
        "Other environments still open from the Environments view (2).",
        "4 services have no built image yet: api/api, docs/docs, web/web and 1 more.",
        "mm build",
        "2  Install the shared development services",
        "mm dev services plan",
        "migrate exited with code 137",
    ] {
        assert!(wide.contains(marker), "missing {marker}:\n{wide}");
    }
    assert!(
        !wide.contains("mm-services-dev-"),
        "no resource ids in a step"
    );

    model.setup.steps[1].ready = false;
    assert!(draw(&model, 140, 40)?.contains("after the steps above"));

    // A short terminal keeps the selected step on screen.
    model.scroll = 1;
    let short = draw(&model, 60, 20)?;
    assert!(
        short.contains("2  Install the shared development"),
        "{short}"
    );
    Ok(())
}

#[test]
fn overview_says_why_an_environment_cannot_be_shown() -> Result<(), Box<dyn std::error::Error>> {
    use crate::presentation::tui::environments::EnvironmentEntry;
    use crate::presentation::tui::setup::SetupView;

    let mut model = remote_model()?;
    model.tab = Tab::Overview;
    model.snapshot.stacks.clear();
    model.environments.entries = ["development", "production"]
        .map(|name| EnvironmentEntry {
            name: name.to_owned(),
            ..EnvironmentEntry::default()
        })
        .to_vec();
    model.setup = SetupView {
        steps: Vec::new(),
        unavailable: Some("ssh: connect to host prod-eu port 22: Connection timed out".to_owned()),
    };
    for (width, height) in SIZES {
        let rendered = draw(&model, width, height)?;
        for marker in ["Cannot show production", "connect to host prod-eu port 22"] {
            assert!(
                rendered.contains(marker),
                "{width}x{height} is missing {marker}:\n{rendered}"
            );
        }
        assert!(!rendered.contains("run step"));
    }
    let wide = draw(&model, 140, 40)?;
    assert!(
        wide.contains("not available"),
        "the header says so on every view"
    );
    assert!(wide.contains("mm keeps checking and shows it as soon as it answers."));
    assert!(wide.contains("Other environments still open from the Environments"));

    // With steps, the steps explain; the raw reason stays in Diagnostics.
    model.setup.steps = setup_view().steps;
    let both = draw(&model, 140, 40)?;
    assert!(both.contains("production cannot be shown until these are done."));
    assert!(!both.contains("Connection timed out"));

    model.environments.switching = Some("production".to_owned());
    model.environments.reloading = true;
    assert!(draw(&model, 140, 40)?.contains("Checking production"));
    Ok(())
}

#[test]
fn guided_view_marks_the_current_step_or_its_outcome() -> Result<(), Box<dyn std::error::Error>> {
    for step in [
        WorkflowStep::ChooseJourney,
        WorkflowStep::Discover,
        WorkflowStep::Configure,
        WorkflowStep::Review,
        WorkflowStep::Applying,
        WorkflowStep::Complete,
        WorkflowStep::Error,
        WorkflowStep::Cancelled,
    ] {
        let mut terminal = Terminal::new(TestBackend::new(80, 16))?;
        let state = WorkflowState {
            step,
            message: Some("semantic-state-marker".to_owned()),
            ..WorkflowState::default()
        };
        terminal.draw(|frame| render_guided_state(frame, frame.area(), &state, 0))?;
        let rendered = buffer_text(terminal.backend().buffer());
        let expected = match step_position(step) {
            Some(position) => format!(
                "◉ {}",
                ["Choose", "Discover", "Configure", "Review", "Apply", "Done"][position]
            ),
            None => format!("{step:?}"),
        };
        assert!(
            rendered.contains(&expected),
            "{step:?} is missing {expected}"
        );
        assert!(rendered.contains("semantic-state-marker"));
    }
    Ok(())
}

#[test]
fn test_backend_small_terminal_has_linear_accessible_fallback()
-> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = Terminal::new(TestBackend::new(30, 8))?;
    terminal.draw(render_small_terminal)?;
    let rendered = buffer_text(terminal.backend().buffer());
    assert!(rendered.contains("Terminal too small"));
    assert!(rendered.contains("mm init"));
    assert!(rendered.contains("provider"));
    Ok(())
}

#[test]
fn test_backend_renders_provider_dashboard_review_and_terminal_states_semantically()
-> Result<(), Box<dyn std::error::Error>> {
    let mut providers = provider_model()?;
    for (screen, marker) in [
        (ProviderScreen::Dashboard, "x external setup"),
        (
            ProviderScreen::Review(Box::new(ProviderReview::Rotation(
                DirectSecretRotationPlan {
                    project_name: "project".to_owned(),
                    environment: "integration".to_owned(),
                    name: "database.password".to_owned(),
                    provider: "age".to_owned(),
                    current_revision: "revision".to_owned(),
                    requires_hidden_input: true,
                    plan_hash: "provider-plan-hash".to_owned(),
                },
            ))),
            "provider-plan-hash",
        ),
        (
            ProviderScreen::Applying("applying-provider-marker".to_owned()),
            "applying-provider-marker",
        ),
        (
            ProviderScreen::Complete("complete-provider-marker".to_owned()),
            "complete-provider-marker",
        ),
        (
            ProviderScreen::Error("error-provider-marker".to_owned()),
            "error-provider-marker",
        ),
        (ProviderScreen::Cancelled, "Cancelled before"),
    ] {
        providers.screen = screen;
        let mut terminal = Terminal::new(TestBackend::new(110, 24))?;
        terminal.draw(|frame| render_providers(frame, frame.area(), &providers, 0))?;
        let rendered = buffer_text(terminal.backend().buffer());
        assert!(
            rendered.contains(marker),
            "missing semantic marker {marker}"
        );
    }
    Ok(())
}

#[test]
fn hidden_input_overlay_renders_only_length_and_survives_small_resize()
-> Result<(), Box<dyn std::error::Error>> {
    const SENTINEL: &str = "hidden-openbao-sentinel-1bd2";
    for (width, height) in [(100, 24), (40, 10)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|frame| {
            render_secret_prompt_overlay(frame, "Developer token", SENTINEL.chars().count());
        })?;
        let rendered = buffer_text(terminal.backend().buffer());
        assert!(rendered.contains("Hidden input"));
        assert!(rendered.contains("never stored"));
        assert!(!rendered.contains(SENTINEL));
    }
    Ok(())
}

/// Writes every view as colored HTML for visual review:
/// `MM_TUI_SNAPSHOT_DIR=<dir> COLORTERM=truecolor cargo test -p mm --features unstable tui_snapshots -- --ignored`
#[test]
#[ignore = "writes review snapshots to MM_TUI_SNAPSHOT_DIR"]
fn tui_snapshots() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::PathBuf::from(std::env::var("MM_TUI_SNAPSHOT_DIR")?);
    std::fs::create_dir_all(&directory)?;
    let mut model = rich_model()?;
    model.scroll = 2;
    for tab in Tab::ALL {
        model.tab = tab;
        for (width, height) in [(140, 40), (90, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| render(frame, &model))?;
            let name = format!("{}-{width}x{height}.html", tab.title().to_lowercase());
            std::fs::write(
                directory.join(name),
                buffer_html(terminal.backend().buffer()),
            )?;
        }
        model.scroll = model.scroll.min(model.selectable_rows().saturating_sub(1));
    }
    model.tab = Tab::Services;
    model.help = true;
    let mut terminal = Terminal::new(TestBackend::new(140, 40))?;
    terminal.draw(|frame| render(frame, &model))?;
    std::fs::write(
        directory.join("help-140x40.html"),
        buffer_html(terminal.backend().buffer()),
    )?;
    for (name, model) in variable_scenes()? {
        for (width, height) in [(120, 40), (80, 24)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| render(frame, &model))?;
            std::fs::write(
                directory.join(format!("{name}-{width}x{height}.html")),
                buffer_html(terminal.backend().buffer()),
            )?;
        }
    }
    for (name, model) in scenes()? {
        for (width, height) in [(140, 32), (90, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| render(frame, &model))?;
            std::fs::write(
                directory.join(format!("{name}-{width}x{height}.html")),
                buffer_html(terminal.backend().buffer()),
            )?;
        }
    }
    Ok(())
}

/// The Variables view of four services: filtered, choosing a service, and searching.
fn variable_scenes() -> Result<Vec<(&'static str, Model)>, Box<dyn std::error::Error>> {
    use crate::presentation::tui::model::VariablesPane;

    let variable =
        |name: &str, source: &str, status: &str, value: Option<&str>, sensitive: bool| {
            serde_json::json!({
                "name": name, "global_key": name, "required": true, "has_default": false,
                "sensitive": sensitive, "source": source, "status": status, "value": value,
                "location": { "path": "/project/app.env", "line": 4, "column": 1 }
            })
        };
    let mut model = rich_model()?;
    model.tab = Tab::Variables;
    model.environment = Some(serde_json::from_value(serde_json::json!({
        "project_name": "fixture", "environment": "development",
        "config_hash": "config-hash", "input_hash": "input-hash", "diagnostics": [],
        "modules": [
            { "module": "backend", "template_path": "/project/app.env", "services": [
                { "stack": "app", "service": "api", "variables": [
                    variable("DATABASE_URL", "config_value", "resolved", Some("postgres://db/app"), false),
                    variable("API_SECRET", "secret", "resolved", Some("hidden"), true),
                    variable("SENTRY_DSN", "missing", "unset", None, false),
                    variable("LOG_LEVEL", "template_default", "defaulted", Some("info"), false),
                    variable("CACHE_URL", "sqlite_override", "resolved", Some("redis://cache:6379/0"), false),
                ]},
                { "stack": "app", "service": "worker", "variables": [
                    variable("DATABASE_URL", "config_value", "resolved", Some("postgres://db/app"), false),
                    variable("QUEUE_URL", "config_value", "resolved", Some("redis://queue:6379/1"), false),
                    variable("SENTRY_DSN", "missing", "unset", None, false),
                    variable("CONCURRENCY", "template_literal", "resolved", Some("8"), false),
                ]},
                { "stack": "app", "service": "migrate", "variables": [
                    variable("DATABASE_URL", "config_value", "resolved", Some("postgres://db/app"), false),
                ]}
            ]},
            { "module": "frontend", "template_path": "/project/web.env", "services": [
                { "stack": "app", "service": "web", "variables": [
                    variable("PUBLIC_API_URL", "config_value", "resolved", Some("https://api.fixture.mm.test"), false),
                    variable("SESSION_KEY", "secret", "resolved", Some("hidden"), true),
                ]},
                { "stack": "app", "service": "notifications-dispatcher", "variables": [
                    variable("SMTP_HOST", "template_default", "defaulted", Some("mailpit"), false),
                    variable("SMTP_PORT", "template_default", "defaulted", Some("1025"), false),
                ]}
            ]}
        ]
    }))?);
    let mut scenes = vec![("scene-variables", model.clone())];
    let mut services = model.clone();
    services.env.pane = VariablesPane::Services;
    services.env.service = 1;
    scenes.push(("scene-variables-services", services.clone()));
    let mut search = services;
    search.env.pane = VariablesPane::Variables;
    search.env.searching = true;
    search.env.filter = "url".to_owned();
    scenes.push(("scene-variables-search", search.clone()));
    search.env.searching = false;
    search.env.filter.clear();
    search.env.problems_only = true;
    search.scroll = 1;
    scenes.push(("scene-variables-problems", search));
    Ok(scenes)
}

/// Named states of the service actions and remote environments, for review snapshots.
fn scenes() -> Result<Vec<(&'static str, Model)>, Box<dyn std::error::Error>> {
    use crate::presentation::tui::panel::{Panel, RunPrompt, menu_for_selection};

    let mut scenes = Vec::new();
    let mut services = rich_model()?;
    services.tab = Tab::Services;
    services.scroll = 3;

    let mut menu = services.clone();
    menu.menu = menu_for_selection(&menu);
    scenes.push(("scene-service-menu", menu));

    let mut logs = services.clone();
    logs.panel = Some(Panel::Logs(sample_logs(&logs)?));
    scenes.push(("scene-logs", logs.clone()));
    if let Some(Panel::Logs(panel)) = &mut logs.panel {
        panel.filter = "error".to_owned();
        panel.timestamps = true;
    }
    scenes.push(("scene-logs-filtered", logs));

    let mut run = services.clone();
    run.panel = Some(Panel::Run(sample_run(&run)?));
    scenes.push(("scene-run", run));

    let mut prompt = services.clone();
    prompt.scroll = 4;
    prompt.run_prompt = prompt.selected_service_target().map(|target| RunPrompt {
        target,
        input: "bin/rails db:migrate:status".to_owned(),
        recalled: None,
        error: None,
    });
    scenes.push(("scene-run-prompt", prompt));

    let mut restart = services.clone();
    restart.scroll = 7;
    crate::presentation::tui::update::update(
        &mut restart,
        crate::presentation::tui::update::Message::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('r'),
            crossterm::event::KeyModifiers::NONE,
        )),
    );
    scenes.push(("scene-restart", restart));

    let mut environments = rich_model()?;
    environments.tab = Tab::Environments;
    environments.environments.probes = remote_model()?.environments.probes;
    environments.environments.probes.remove("development");
    environments.environments.probes.insert(
        "production".to_owned(),
        crate::presentation::tui::environments::ProbeState::Ready {
            probe: crate::presentation::tui::environments::EnvironmentProbe {
                services: environments
                    .services()
                    .iter()
                    .map(|service| crate::presentation::tui::ServiceLine::from(*service))
                    .collect(),
                routes: 2,
                issues: 2,
                principal: Some("alice".to_owned()),
                access: Some("deploy +exec[api]".to_owned()),
                deployed_commit: Some("9f2c1e7ab4d03c55e1".to_owned()),
                nodes: Vec::new(),
            },
            at: std::time::Instant::now(),
        },
    );
    environments.scroll = 2;
    scenes.push(("scene-environments", environments.clone()));
    environments.environments.switching = Some("production".to_owned());
    scenes.push(("scene-switching", environments));

    // A local environment that still needs a build and the shared services.
    let mut setup = rich_model()?;
    setup.snapshot.stacks.clear();
    setup.snapshot.ingress.clear();
    setup.snapshot.validation.diagnostics.clear();
    setup.setup = setup_view();
    scenes.push(("scene-setup", setup.clone()));
    // A first open on a new machine: nothing could be read yet.
    let issue = crate::domain::readiness::SetupIssue::MissingTarget {
        name: "local".to_owned(),
        path: "/mm/targets.yaml".into(),
    };
    setup.setup.steps.insert(
        0,
        crate::presentation::tui::setup::SetupStep {
            title: issue.title(),
            detail: issue.summary(),
            command: Some("mm target local".to_owned()),
            code: Some(issue.diagnostic_code()),
            ready: true,
            issue: Some(issue),
        },
    );
    setup.setup.steps[1].ready = false;
    setup.setup.unavailable = Some("target 'local' is not configured".to_owned());
    scenes.push(("scene-setup-first-open", setup));

    let mut remote = remote_model()?;
    remote.tab = Tab::Overview;
    remote.snapshot.stacks.clear();
    remote.snapshot.ingress.clear();
    remote.setup.unavailable =
        Some("ssh: connect to host prod-eu.example.com port 22: Connection timed out".to_owned());
    scenes.push(("scene-unavailable", remote));

    let reading = reading_model()?;
    scenes.push(("scene-reading", reading.clone()));
    let mut partly = reading;
    partly.tab = Tab::Services;
    crate::presentation::tui::loading::apply(&mut partly, one_node_answered()?);
    scenes.push(("scene-reading-services", partly));

    let mut remote = remote_model()?;
    remote.tab = Tab::Environments;
    remote.scroll = 2;
    scenes.push(("scene-remote-environments", remote.clone()));
    remote.tab = Tab::Services;
    remote.scroll = 3;
    scenes.push(("scene-remote-services", remote.clone()));
    remote.menu = menu_for_selection(&remote);
    scenes.push(("scene-remote-service-menu", remote.clone()));
    remote.menu = None;
    remote.tab = Tab::Variables;
    remote.scroll = 0;
    scenes.push(("scene-remote-variables", remote.clone()));
    remote.tab = Tab::Overview;
    scenes.push(("scene-remote-overview", remote.clone()));

    let mut cluster = cluster_model()?;
    cluster.tab = Tab::Environments;
    cluster.scroll = 2;
    scenes.push(("scene-cluster-environments", cluster.clone()));
    cluster.tab = Tab::Overview;
    scenes.push(("scene-cluster-overview", cluster.clone()));
    cluster.tab = Tab::Services;
    cluster.scroll = 0;
    scenes.push(("scene-cluster-services", cluster.clone()));
    cluster.panel = Some(Panel::Logs(sample_cluster_logs(&cluster)?));
    scenes.push(("scene-cluster-logs", cluster.clone()));
    cluster.panel = None;
    // The row of one node, which the selection rests on.
    cluster.scroll = 2;
    scenes.push(("scene-cluster-node", cluster.clone()));
    cluster.scroll = 0;
    for (name, scene) in deploy_scenes(&cluster) {
        scenes.push((name, scene));
    }

    remote.tab = Tab::Access;
    remote.access.team = crate::presentation::tui::fixtures::team();
    remote.scroll = 1;
    scenes.push(("scene-team", remote.clone()));
    for (name, editor) in sample_editors(&remote)? {
        let mut scene = remote.clone();
        if name == "scene-environment-form" {
            scene.tab = Tab::Environments;
        }
        scene.editor = Some(editor);
        scenes.push((name, scene));
    }
    Ok(scenes)
}

/// What `mm deploy plan` prints for a cluster deployment, as the review
/// shows it.
fn sample_plan_lines() -> Vec<String> {
    [
        "Cluster deployment plan 8a41c0de91f2a7b35c0e19d4f7a6b2c18e3d5f90a7c4b1e2d6f8a3c5b7e9d1f4",
        "  target:   prod-eu (app-1, app-2, app-3)",
        "  commit:   41c07de92ab3c5e8f1d2a4b6c8e0f2a4b6c8d0e1",
        "  expires:  2026-11-14T16:42:07Z",
        "  moves:",
        "    app/api: starts on app-2, then stops on app-1",
        "  rollout: app on app-1, then app-2, then app-3",
        "           a stack may take 10m on a node to become healthy (deploy.convergence-timeout); the next node starts once the others resolve its containers",
        "           on failure every node this deployment changed returns to what it ran before",
        "",
        "Node app-1  mmd 0.1.0  section 2d7e1f0c9a3b5d7f9e1c3a5b7d9f1e3c5a7b9d1f3e5c7a9b1d3f5e7c9a1b3d5f",
        "  access:   unchanged (3 user(s))",
        "  services: app/api, app/web",
        "  stops here: app/api, after it runs on app-2 (last step of the apply)",
        "  restart: unless-stopped added to app/web (no `restart:` in the Compose file), so it comes back when the node reboots; set `restart: \"no\"` on a service to leave it stopped",
        "Deployment plan 6cc69274ac7133243fddae5494a52b31509d400d19339b04800dee51382c625c",
        "  shop [production] -> prod-eu",
        "  environment delivery: direct (from built-in-default)",
        "  stack:  app (9f2c1e7ab4d0)",
        "    image: frontend/web -> acme/web:latest",
        "  routes: unchanged",
        "  ingress: no desired routes",
    ]
    .map(str::to_owned)
    .to_vec()
}

/// The deploy review of a cluster, and the same plan halfway through its
/// apply.
fn deploy_scenes(cluster: &Model) -> Vec<(&'static str, Model)> {
    use crate::domain::rollout::{ProgressStep, RolloutProgress};
    use crate::presentation::tui::control::{
        StackOperation, StackOutcome, StartReview, apply_outcome,
    };

    let mut review = cluster.clone();
    let plan = StartReview {
        plan_hash: "8a41c0de91f2a7b35c0e19d4f7a6b2c18e3d5f90a7c4b1e2d6f8a3c5b7e9d1f4".to_owned(),
        target: "prod-eu".to_owned(),
        stacks: vec!["app".to_owned()],
        routes: 0,
        remote: true,
        nodes: vec!["app-1".to_owned(), "app-2".to_owned(), "app-3".to_owned()],
        lines: sample_plan_lines(),
        rollback: None,
    };
    apply_outcome(&mut review, StackOutcome::Reviewed(Ok(plan.clone())));
    let mut applying = review.clone();
    applying.busy = Some(StackOperation::Start(plan.plan_hash));
    if let Some(shown) = applying.review.as_mut() {
        shown.stage = crate::presentation::tui::review::ReviewStage::Applying;
    }
    for (node, step) in [
        ("app-1", ProgressStep::Preparing),
        ("app-1", ProgressStep::Stack("app".to_owned())),
        ("app-1", ProgressStep::Healthy("app".to_owned())),
        ("app-2", ProgressStep::Preparing),
        ("app-2", ProgressStep::Stack("app".to_owned())),
    ] {
        apply_outcome(
            &mut applying,
            StackOutcome::Progress(RolloutProgress::new(node, step)),
        );
    }
    vec![
        ("scene-cluster-deploy-review", review),
        ("scene-cluster-deploy-progress", applying),
    ]
}

const SAMPLE_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGJ5dGVzLW9mLWEtdGVzdC1rZXktMDEyMzQ1Njc4OWFi carol@laptop";

/// The teammate flow at each stage, and an environment form.
fn sample_editors(
    model: &Model,
) -> Result<Vec<(&'static str, crate::presentation::tui::editor::Editor)>, Box<dyn std::error::Error>>
{
    use crate::domain::onboarding::{ConfigMutationAction, ConfigMutationPlan};
    use crate::presentation::tui::editor::{
        Editor, EditorContext, EditorKind, EditorOpen, EditorStage,
    };

    let context = EditorContext::from_model(model);
    let mut form = Editor::new(
        EditorOpen {
            kind: EditorKind::Teammate,
            action: ConfigMutationAction::Add,
            name: None,
            parent: None,
        },
        None,
        &context,
    );
    for (id, value) in [
        ("name", "carol"),
        ("key", SAMPLE_KEY),
        ("access:staging", "deploy"),
        ("access:production", "view"),
    ] {
        if let Some(field) = form.fields.iter_mut().find(|field| field.id == id) {
            value.clone_into(&mut field.value);
        }
    }
    form.focus = 3;
    let request = form.request()?;
    let mut review = form.clone();
    review.stage = EditorStage::Review {
        plan: Box::new(ConfigMutationPlan {
            request: request.clone(),
            affected_files: vec!["/project/.config/mm/users.yaml".into()],
            canonical_diff: format!(
                "--- .config/mm/users.yaml\n+++ .config/mm/users.yaml\n@@ -6,3 +6,9 @@\n bob:\n   keys:\n     - ssh-ed25519 AAAAC3Nza… bob@desktop\n   access:\n     \"*\": view\n+carol:\n+  keys:\n+    - {SAMPLE_KEY}\n+  access:\n+    production: view\n+    staging: deploy\n"
            ),
            generated_resources: vec!["user:carol".to_owned()],
            generated_routes: Vec::new(),
            privilege_prompts: Vec::new(),
            source_hash: "source".to_owned(),
            result_hash: "result".to_owned(),
            plan_hash: "5b1f0c2a9d3e47f18a6c".to_owned(),
        }),
        scroll: 0,
    };
    let mut done = form.clone();
    done.stage = EditorStage::Done {
        headline: "Added teammate 'carol' in .config/mm/users.yaml.".to_owned(),
        steps: done.next_steps(&request, ".config/mm/users.yaml", model.remote()),
    };
    let mut environment = Editor::new(
        EditorOpen {
            kind: EditorKind::Environment,
            action: ConfigMutationAction::Edit,
            name: Some("staging".to_owned()),
            parent: None,
        },
        Some(serde_json::json!({
            "target": "staging-eu",
            "runtime": { "driver": "docker-compose" },
            "deploy-from": ["main"]
        })),
        &context,
    );
    environment.focus = 1;
    Ok(vec![
        ("scene-teammate-form", form),
        ("scene-teammate-review", review),
        ("scene-teammate-done", done),
        ("scene-environment-form", environment),
    ])
}

#[test]
fn the_editor_shows_its_form_its_diff_and_the_next_steps() -> Result<(), Box<dyn std::error::Error>>
{
    let mut model = remote_model()?;
    model.tab = Tab::Access;
    model.access.team = crate::presentation::tui::fixtures::team();
    let team = draw(&model, 140, 40)?;
    for marker in ["bob", "ci", "1 key · automation", "3 members", "D remove"] {
        assert!(team.contains(marker), "missing {marker:?} in:\n{team}");
    }
    for (name, editor) in sample_editors(&model)? {
        model.editor = Some(editor);
        let markers: &[&str] = match name {
            "scene-teammate-form" => &[
                "Add a teammate",
                "Public key *",
                "staging",
                "‹ view ›",
                "operate: + restart and jobs",
                "enter review",
                "takes effect once it is merged and deployed",
            ],
            "scene-teammate-review" => &[
                "Review: Add a teammate",
                "+carol:",
                "plan 5b1f0c2a9d3e",
                "Writes users.yaml",
                "enter/y apply",
            ],
            "scene-teammate-done" => &[
                "Added teammate 'carol'",
                "WHAT HAPPENS NEXT",
                "open a pull request",
                "mm team sync",
                "carol opens an environment",
            ],
            _ => &[
                "Edit environment staging",
                "‹ docker-compose ›",
                "Deploys from",
            ],
        };
        for (width, height) in SIZES {
            let rendered = draw(&model, width, height)?;
            assert!(
                rendered.contains(markers[0]),
                "{name} {width}x{height} lost its title in:\n{rendered}"
            );
        }
        let rendered = draw(&model, 140, 40)?;
        for marker in markers {
            assert!(
                rendered.contains(marker),
                "{name} is missing {marker:?} in:\n{rendered}"
            );
        }
    }
    Ok(())
}

fn sample_logs(
    model: &Model,
) -> Result<crate::presentation::tui::panel::LogsPanel, Box<dyn std::error::Error>> {
    use crate::presentation::tui::panel::{LogLine, LogsPanel, StreamState};

    let target = model
        .selected_service_target()
        .ok_or("no service selected")?;
    let mut logs = LogsPanel::new(1, target);
    logs.state = StreamState::Streaming;
    for (index, (stderr, text)) in [
        (false, "Booting api (rev 9f2c1e7) in production mode"),
        (false, "Connected to postgres://db/app (pool 10)"),
        (false, "Listening on http://0.0.0.0:8080"),
        (false, "GET /healthz 200 1ms"),
        (false, "GET /api/projects 200 18ms request_id=4f1c"),
        (false, "POST /api/deployments 201 142ms request_id=4f1d"),
        (
            true,
            "WARN slow query (812ms): SELECT * FROM deployments WHERE project_id = $1",
        ),
        (false, "GET /api/deployments/118 200 9ms request_id=4f1e"),
        (
            true,
            "ERROR upstream worker timed out after 30s request_id=4f1f",
        ),
        (true, "    at Queue.dispatch (/app/src/queue.js:88:13)"),
        (
            true,
            "    at async Deployments.create (/app/src/deployments.js:41:5)",
        ),
        (false, "POST /api/deployments 502 30004ms request_id=4f1f"),
        (false, "\u{1b}[32mGET\u{1b}[0m /healthz 200 1ms"),
        (false, "GET /api/projects 200 16ms request_id=4f20"),
        (false, "Worker reconnected; draining 3 queued jobs"),
        (false, "GET /healthz 200 1ms"),
    ]
    .into_iter()
    .enumerate()
    {
        logs.push(LogLine {
            timestamp: Some(format!("2026-11-12T14:03:{:02}.000000000Z", index * 3)),
            stderr,
            text: text.to_owned(),
            ..LogLine::default()
        });
    }
    Ok(logs)
}

/// The log of `web` on a cluster: lines of three nodes in time order, one of
/// them on standard error, and a node that stopped answering.
fn sample_cluster_logs(
    model: &Model,
) -> Result<crate::presentation::tui::panel::LogsPanel, Box<dyn std::error::Error>> {
    use crate::presentation::tui::panel::{LogLine, LogsPanel, StreamState};

    let target = model
        .selected_service_target()
        .ok_or("no service selected")?;
    let mut logs = LogsPanel::new(1, target);
    logs.state = StreamState::Streaming;
    for (index, (node, stderr, text)) in [
        ("app-1", false, "GET / 200 12ms request_id=7a10"),
        (
            "app-2",
            false,
            "GET /assets/app.css 200 3ms request_id=7a11",
        ),
        ("app-1", false, "GET /healthz 200 1ms"),
        ("app-2", false, "GET /healthz 200 1ms"),
        ("app-1", false, "POST /session 302 41ms request_id=7a12"),
        (
            "app-2",
            true,
            "WARN upstream api answered after 2.4s request_id=7a13",
        ),
        ("app-2", false, "GET /projects 200 2412ms request_id=7a13"),
        ("app-1", false, "GET /projects/118 200 22ms request_id=7a14"),
        (
            "app-1",
            true,
            "ERROR could not render /reports: template missing",
        ),
        ("app-1", false, "GET /reports 500 6ms request_id=7a15"),
        ("app-2", false, "GET /healthz 200 1ms"),
    ]
    .into_iter()
    .enumerate()
    {
        logs.push(LogLine {
            timestamp: Some(format!("2026-11-12T14:03:{:02}.000000000Z", index * 2)),
            stderr,
            node: Some(node.to_owned()),
            container: Some("web-1".to_owned()),
            text: text.to_owned(),
            ..LogLine::default()
        });
    }
    logs.push(LogLine {
        node: Some("app-3".to_owned()),
        about_node: true,
        text: "no answer after 5 seconds; the other nodes' lines are shown without it".to_owned(),
        ..LogLine::default()
    });
    Ok(logs)
}

fn sample_run(
    model: &Model,
) -> Result<crate::presentation::tui::panel::RunPanel, Box<dyn std::error::Error>> {
    use crate::presentation::tui::panel::RunPanel;

    let target = model.exec_target(
        model
            .selected_service_target()
            .ok_or("no service selected")?,
    );
    let mut run = RunPanel::new(
        2,
        target,
        "bin/rails db:migrate:status".to_owned(),
        vec!["bin/rails".to_owned(), "db:migrate:status".to_owned()],
    );
    run.push(
        false,
        "database: app_production\n\n Status   Migration ID    Migration Name\n--------------------------------------------------\n   up     20261101093000  Create projects\n   up     20261104120500  Add deployments\n",
    );
    run.push(true, "warning: 1 migration is pending\n");
    run.push(false, "  down    20261110081500  Add receipts\n");
    run.finish(Ok(0));
    Ok(run)
}

#[test]
fn services_view_nests_containers_and_opens_the_action_menu()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::presentation::tui::panel::menu_for_selection;

    let mut model = rich_model()?;
    model.tab = Tab::Services;
    let rendered = draw(&model, 170, 48)?;
    for marker in [
        "▾ web",
        "├ fixture-web-1",
        "└ fixture-api-3",
        "Exited (137)",
        "unhealthy",
        "enter actions",
        "ACTIONS",
        "https://web.fixture.mm.test",
    ] {
        assert!(
            rendered.contains(marker),
            "missing {marker:?} in:\n{rendered}"
        );
    }
    model.collapsed.insert(("app".to_owned(), "web".to_owned()));
    let folded = draw(&model, 170, 48)?;
    assert!(folded.contains("▸ web"));
    assert!(!folded.contains("fixture-web-2"));

    model.scroll = 3;
    model.menu = menu_for_selection(&model);
    for (width, height) in SIZES {
        let rendered = draw(&model, width, height)?;
        for marker in ["Logs", "Run a command", "Restart", "Variables"] {
            assert!(rendered.contains(marker), "{width}x{height} lost {marker}");
        }
    }
    let menu = draw(&model, 140, 40)?;
    assert!(menu.contains("this container only"));
    assert!(menu.contains("esc close"));

    model.menu = None;
    model.restarting.push("api".to_owned());
    assert!(draw(&model, 140, 40)?.contains("restarting"));
    Ok(())
}

#[test]
fn log_and_command_panels_show_output_state_and_filters() -> Result<(), Box<dyn std::error::Error>>
{
    use crate::presentation::tui::panel::{Panel, RunPrompt, StreamState};

    let mut model = rich_model()?;
    model.tab = Tab::Services;
    model.scroll = 3;
    let mut logs = sample_logs(&model)?;
    model.panel = Some(Panel::Logs(logs.clone()));
    for (width, height) in SIZES {
        let rendered = draw(&model, width, height)?;
        assert!(rendered.contains("Logs · api"), "{width}x{height}");
        assert!(
            rendered.contains("GET /healthz 200 1ms"),
            "{width}x{height}"
        );
    }
    let rendered = draw(&model, 140, 40)?;
    for marker in [
        "following",
        "16 lines",
        "development · local",
        "space pause",
    ] {
        assert!(
            rendered.contains(marker),
            "missing {marker:?} in:\n{rendered}"
        );
    }
    assert!(
        !rendered.contains("WORKSPACE"),
        "the panel covers the sidebar"
    );
    assert!(!rendered.contains("[32m"), "escape sequences are removed");
    assert!(!rendered.contains("14:03:00"));

    logs.timestamps = true;
    logs.filter = "ERROR".to_owned();
    logs.scroll_back = 0;
    model.panel = Some(Panel::Logs(logs.clone()));
    let rendered = draw(&model, 140, 40)?;
    assert!(rendered.contains("1 of 16 lines"));
    assert!(rendered.contains("14:03:24"));
    assert!(!rendered.contains("Listening on"));

    logs.filter.clear();
    logs.scroll_back = 4;
    logs.unseen = 2;
    model.panel = Some(Panel::Logs(logs.clone()));
    let rendered = draw(&model, 140, 40)?;
    assert!(rendered.contains("paused · 2 new"));
    assert!(rendered.contains("space follow"));

    logs.state = StreamState::Failed("the host closed the stream".to_owned());
    model.panel = Some(Panel::Logs(logs));
    let rendered = draw(&model, 140, 40)?;
    assert!(rendered.contains("stream failed"));
    assert!(rendered.contains("the host closed the stream"));
    assert!(rendered.contains("r reconnect"));

    model.panel = Some(Panel::Run(sample_run(&model)?));
    let rendered = draw(&model, 140, 40)?;
    for marker in [
        "Run · fixture-api-1",
        "$ bin/rails db:migrate:status",
        "exit 0",
        "Add receipts",
        "warning: 1 migration is pending",
        "enter run another",
    ] {
        assert!(
            rendered.contains(marker),
            "missing {marker:?} in:\n{rendered}"
        );
    }

    model.run_prompt = Some(RunPrompt {
        target: model.exec_target(
            model
                .selected_service_target()
                .ok_or("no service selected")?,
        ),
        input: "printenv".to_owned(),
        recalled: None,
        error: Some("The command has an unclosed ' quote.".to_owned()),
    });
    for (width, height) in SIZES {
        let rendered = draw(&model, width, height)?;
        assert!(rendered.contains("Run a command"), "{width}x{height}");
        assert!(rendered.contains("printenv"), "{width}x{height}");
    }
    assert!(draw(&model, 140, 40)?.contains("unclosed ' quote"));
    Ok(())
}

#[test]
fn environments_view_shows_where_each_runs_and_how_it_is_doing()
-> Result<(), Box<dyn std::error::Error>> {
    let mut model = remote_model()?;
    model.tab = Tab::Environments;
    for (width, height) in SIZES {
        let rendered = draw(&model, width, height)?;
        for marker in ["development", "staging", "production", "preview"] {
            assert!(rendered.contains(marker), "{width}x{height} lost {marker}");
        }
    }
    model.scroll = 2;
    let rendered = draw(&model, 170, 48)?;
    for marker in [
        " production ",
        "remote · prod-eu",
        "current",
        "default",
        "2/2 running",
        "2/4 running",
        "unreachable",
        "not set up",
        "docker compose",
        "deploy@prod.example.test:22",
        "Deploys from release",
        "alice  deploy +exec[api]",
        "9f2c1e7ab4d0",
        "4 environments · 2 remote",
    ] {
        assert!(
            rendered.contains(marker),
            "missing {marker:?} in:\n{rendered}"
        );
    }
    model.scroll = 1;
    let failed = draw(&model, 170, 48)?;
    assert!(failed.contains("The last check failed"));
    assert!(failed.contains("Connection timed out"));
    assert!(failed.contains("enter switch anyway"));
    model.scroll = 3;
    assert!(draw(&model, 170, 48)?.contains("preview-host"));

    model.environments.switching = Some("staging".to_owned());
    for (width, height) in SIZES {
        let rendered = draw(&model, width, height)?;
        assert!(
            rendered.contains("Switching to staging"),
            "{width}x{height}"
        );
    }
    assert!(draw(&model, 140, 40)?.contains("deploy@staging.example.test:22"));
    Ok(())
}

#[test]
fn remote_environment_views_describe_the_host_not_this_machine()
-> Result<(), Box<dyn std::error::Error>> {
    let mut model = remote_model()?;
    model.tab = Tab::Services;
    let rendered = draw(&model, 170, 48)?;
    for marker in [
        "Target",
        "you are alice",
        "deploy +exec[api]",
        "deployed 9f2c1e7ab4d0",
        "u deploy",
        "unavailable",
    ] {
        assert!(
            rendered.contains(marker),
            "missing {marker:?} in:\n{rendered}"
        );
    }
    assert!(!rendered.contains("Control plane"));
    assert!(!rendered.contains("d stop"));

    model.tab = Tab::Variables;
    let rendered = draw(&model, 170, 48)?;
    assert!(rendered.contains("from this checkout"));
    assert!(rendered.contains("enter set in config"));
    assert!(!rendered.contains("remove override"));
    if let Some(environment) = model.environment.as_mut() {
        environment.modules[0].services[0].variables[0].global_key =
            Some("DATABASE_URL".to_owned());
    }
    assert!(draw(&model, 170, 48)?.contains("commit and deploy to apply"));

    model.environment = None;
    assert!(draw(&model, 170, 48)?.contains("differs from what the host has deployed"));

    model.tab = Tab::Overview;
    let rendered = draw(&model, 170, 48)?;
    assert!(rendered.contains("Runs on"));
    assert!(rendered.contains("2 environments"));
    Ok(())
}

fn buffer_html(buffer: &Buffer) -> String {
    use ratatui::style::{Color, Modifier};
    let css = |color: Color, fallback: &str| match color {
        Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
        Color::Black => "#000".to_owned(),
        _ => fallback.to_owned(),
    };
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><body style=\"margin:0;background:#0b0d0f\"><pre style=\"margin:16px;font:14px/1.25 'JetBrains Mono','DejaVu Sans Mono',monospace;color:#d8dcda\">",
    );
    for y in buffer.area.top()..buffer.area.bottom() {
        for x in buffer.area.left()..buffer.area.right() {
            let cell = &buffer[(x, y)];
            let mut foreground = css(cell.fg, "inherit");
            let mut background = css(cell.bg, "transparent");
            if cell.modifier.contains(Modifier::REVERSED) {
                std::mem::swap(&mut foreground, &mut background);
            }
            let weight = if cell.modifier.contains(Modifier::BOLD) {
                700
            } else {
                400
            };
            let symbol = match cell.symbol() {
                "<" => "&lt;",
                ">" => "&gt;",
                "&" => "&amp;",
                other => other,
            };
            html.push_str(&format!(
                "<span style=\"display:inline-block;width:1ch;overflow:visible;color:{foreground};background:{background};font-weight:{weight}\">{symbol}</span>"
            ));
        }
        html.push('\n');
    }
    html.push_str("</pre>");
    html
}

fn buffer_text(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut output = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            output.push_str(buffer[(x, y)].symbol());
        }
        output.push('\n');
    }
    output
}

fn provider_model() -> Result<ProviderModel, Box<dyn std::error::Error>> {
    let driver = crate::adapters::secrets::openbao_provider_descriptor()?;
    let external_name = crate::domain::secret_backend::ProviderInstanceName::new("team-secrets")
        .map_err(std::io::Error::other)?;
    let managed_name = crate::domain::secret_backend::ProviderInstanceName::new("local-secrets")
        .map_err(std::io::Error::other)?;
    let external_configuration = crate::domain::secret_backend::ProviderConfiguration::External {
        address: "https://provider.example.test".to_owned(),
        namespace: None,
        storage_mount: "mm-kv".to_owned(),
        automation_auth_mount: "mm-approle".to_owned(),
        trust_bundle: "ca.pem".into(),
        expected_trust_sha256: format!("sha256:{}", "a".repeat(64)),
    };
    let managed_configuration = crate::domain::secret_backend::ProviderConfiguration::Managed {
        installation: Some("local-secrets".to_owned()),
        release: "2.4".to_owned(),
        management_port: Some(18_200),
        nodes: None,
    };
    let external = crate::domain::secret_backend::ProviderInstanceDescriptor {
        name: external_name.clone(),
        driver: driver.driver.clone(),
        driver_display_name: driver.display_name.clone(),
        mode: crate::domain::secret_backend::ProviderMode::External,
        scope: crate::domain::secret_backend::ProviderScope::Project,
        operations: driver
            .mode(crate::domain::secret_backend::ProviderMode::External)
            .ok_or("external mode")?
            .operations
            .clone(),
        configuration: serde_json::to_value(&external_configuration)?,
    };
    let managed = crate::domain::secret_backend::ProviderInstanceDescriptor {
        name: managed_name.clone(),
        driver: driver.driver.clone(),
        driver_display_name: driver.display_name.clone(),
        mode: crate::domain::secret_backend::ProviderMode::Managed,
        scope: crate::domain::secret_backend::ProviderScope::Installation,
        operations: driver
            .mode(crate::domain::secret_backend::ProviderMode::Managed)
            .ok_or("managed mode")?
            .operations
            .clone(),
        configuration: serde_json::to_value(&managed_configuration)?,
    };
    Ok(ProviderModel::new(SecretBackendTuiContext {
        root: "/project".into(),
        target: "local".to_owned(),
        config_hash: "a".repeat(64),
        project_id: ProjectId(uuid::Uuid::parse_str(
            "019c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?),
        environment: "integration".to_owned(),
        external_connections: vec![ConnectionScope {
            instance: external,
            configuration: external_configuration,
            name: external_name.to_string(),
            locations: vec!["database/password".to_owned()],
        }],
        managed_connections: vec![ConnectionScope {
            instance: managed,
            configuration: managed_configuration,
            name: managed_name.to_string(),
            locations: vec!["application/key".to_owned()],
        }],
        rotatable_secrets: vec!["database.password".to_owned()],
        drivers: vec![driver],
        instances: std::collections::BTreeMap::new(),
    }))
}

#[test]
fn environment_search_edit_and_confirm_overlays_render() -> Result<(), Box<dyn std::error::Error>> {
    use crate::presentation::tui::control::StackOperation;
    use crate::presentation::tui::model::{Confirm, EnvironmentEdit, Notice};
    use crate::presentation::tui::update::Action;

    let mut model = rich_model()?;
    model.tab = Tab::Variables;
    model.env.filter = "sentry".to_owned();
    model.env.searching = true;
    model.env.pending_restart = true;
    let rendered = draw(&model, 120, 40)?;
    assert!(rendered.contains("sentry"));
    assert!(rendered.contains("SENTRY_DSN"));
    assert!(!rendered.contains("DATABASE_URL"));
    assert!(rendered.contains("1 of 3 variables"));
    assert!(rendered.contains("restart to apply"));

    model.env.filter = "nothing-matches".to_owned();
    assert!(draw(&model, 120, 40)?.contains("No variables match"));

    model.env.filter.clear();
    model.env.searching = false;
    model.env.edit = Some(EnvironmentEdit {
        key: "DATABASE_URL".to_owned(),
        value: "postgres://edited".to_owned(),
        usages: 2,
    });
    let rendered = draw(&model, 120, 40)?;
    assert!(rendered.contains("Set override"));
    assert!(rendered.contains("postgres://edited"));
    assert!(rendered.contains("used by 2 variables"));

    model.env.edit = None;
    model.busy = Some(StackOperation::Stop);
    model.confirm = Some(Confirm {
        title: "Stop the stack?".to_owned(),
        lines: vec!["Removes the containers of app.".to_owned()],
        accept: "stop",
        destructive: true,
        action: Action::Stack(StackOperation::Stop),
    });
    for (width, height) in SIZES {
        let rendered = draw(&model, width, height)?;
        assert!(rendered.contains("Stop the stack?"), "{width}x{height}");
    }
    assert!(draw(&model, 120, 40)?.contains("Stopping stack"));

    model.confirm = None;
    model.notice = Some(Notice::success("Stopped app."));
    assert!(draw(&model, 120, 40)?.contains("Stopped app."));
    Ok(())
}

#[test]
fn access_tab_shows_location_identity_team_and_forwards() -> Result<(), Box<dyn std::error::Error>>
{
    use crate::domain::host_policy::{AccessLevel, EnvironmentAccess, ServiceSelection};
    use crate::presentation::tui::model::{AccessView, TeamMemberView};

    let mut model = rich_model()?;
    model.tab = Tab::Access;
    let local = draw(&model, 120, 40)?;
    assert!(local.contains("local"), "{local}");
    assert!(local.contains("j creates one"), "{local}");
    assert!(local.contains("Nobody is in the users file yet"), "{local}");
    assert!(local.contains("a add teammate"), "{local}");
    assert!(local.contains("j request access"), "{local}");

    model.access = AccessView {
        remote_target: Some("prod-eu".to_owned()),
        device_key: Some("SHA256:device".to_owned()),
        identity: Some(crate::domain::remote::RemoteIdentity {
            principal: "alice".to_owned(),
            principal_id: crate::domain::access::PrincipalId(uuid::Uuid::from_u128(1)),
            key_fingerprint: "SHA256:device".to_owned(),
            break_glass: false,
            environment: "staging".to_owned(),
            access: Some(EnvironmentAccess {
                level: AccessLevel::Deploy,
                exec: ServiceSelection::All(false),
                forward: ServiceSelection::Only(["api".to_owned()].into()),
                secrets: false,
            }),
            access_commit: Some("a".repeat(40)),
            deployed_commit: None,
        }),
        identity_error: None,
        team: vec![TeamMemberView {
            name: "alice".to_owned(),
            access: Some("deploy +forward[api]".to_owned()),
            keys: 1,
            machine: false,
            you: true,
        }],
        forwards: vec!["web  127.0.0.1:18081 -> staging/api:8080  auto".to_owned()],
    };
    let remote = draw(&model, 120, 40)?;
    for marker in [
        "remote · prod-eu",
        "SHA256:device",
        "deploy +forward[api]",
        "aaaaaaaaaaaa",
        "nothing yet",
        "alice  you",
        "ACCESS HERE",
        "1 member",
        "18081",
    ] {
        assert!(remote.contains(marker), "missing {marker:?} in:\n{remote}");
    }
    assert!(
        !remote.contains("request access"),
        "this device is already listed"
    );
    Ok(())
}

/// The first frame of a cluster environment: the configuration is known,
/// nothing was asked of the nodes yet.
fn reading_model() -> Result<Model, Box<dyn std::error::Error>> {
    use crate::domain::workspace::NodeStatus;
    use crate::presentation::tui::loading::Loading;

    let mut model = remote_model()?;
    model.tab = Tab::Overview;
    model.snapshot.stacks.clear();
    model.snapshot.ingress.clear();
    model.snapshot.validation.diagnostics.clear();
    model.snapshot.runtime.engine_version = None;
    model.snapshot.runtime.compose_provider = None;
    model.snapshot.runtime.rootless = None;
    model.snapshot.nodes = ["app-1", "app-2", "app-3"]
        .map(|name| NodeStatus::pending(name.to_owned()))
        .to_vec();
    model.environment = None;
    model.tls = None;
    model.access.identity = None;
    model.loading = Loading::everything();
    Ok(model)
}

/// What arrives when the first of three nodes has answered.
fn one_node_answered() -> Result<crate::presentation::tui::Loaded, Box<dyn std::error::Error>> {
    use crate::domain::runtime::ServiceOnNode;
    use crate::domain::workspace::NodeStatus;

    let mut snapshot = remote_model()?.snapshot;
    for stack in &mut snapshot.stacks {
        for service in &mut stack.services {
            service.nodes = vec![ServiceOnNode {
                node: "app-1".to_owned(),
                state: service.state,
                starting: service.starting,
                desired_count: service.desired_count,
                running_count: service.running_count,
                healthy_count: service.healthy_count,
            }];
            for container in &mut service.containers {
                container.node = Some("app-1".to_owned());
            }
        }
    }
    snapshot.nodes = vec![
        NodeStatus {
            reachable: true,
            ..NodeStatus::pending("app-1".to_owned())
        },
        NodeStatus::pending("app-2".to_owned()),
        NodeStatus::pending("app-3".to_owned()),
    ];
    Ok(crate::presentation::tui::Loaded::Status {
        snapshot: Box::new(snapshot),
        complete: false,
    })
}

#[test]
fn a_session_still_being_read_says_so_instead_of_looking_empty()
-> Result<(), Box<dyn std::error::Error>> {
    let mut model = reading_model()?;
    let overview = draw(&model, 140, 40)?;
    assert!(overview.contains("reading"), "{overview}");
    assert!(overview.contains("Asking prod-eu"), "{overview}");
    // Every node is listed, none has answered.
    assert_eq!(
        overview.matches("not answered yet").count(),
        3,
        "{overview}"
    );
    // Nothing is claimed about an environment that has not answered.
    for verdict in [
        "no issues",
        "Everything looks good",
        "The stack is not running",
        "unavailable",
        "● live",
    ] {
        assert!(!overview.contains(verdict), "{verdict}: {overview}");
    }
    for (tab, absent) in [
        (Tab::Services, "No services running"),
        (Tab::Variables, "No variables"),
        (Tab::Ingress, "No routes"),
    ] {
        model.tab = tab;
        let rendered = draw(&model, 140, 40)?;
        assert!(rendered.contains("Asking prod-eu") || rendered.contains("Resolving variables"));
        assert!(!rendered.contains(absent), "{rendered}");
    }
    // What the configuration says is there from the first frame.
    model.tab = Tab::Access;
    let access = draw(&model, 140, 40)?;
    assert!(access.contains("Asking prod-eu who you are"), "{access}");
    model.tab = Tab::Environments;
    assert!(draw(&model, 140, 40)?.contains("production"));
    Ok(())
}

#[test]
fn nodes_fill_in_as_they_answer_and_a_silent_one_is_marked()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::presentation::tui::Loaded;
    use crate::presentation::tui::loading::{Part, apply};

    let mut model = reading_model()?;
    model.tab = Tab::Services;
    apply(&mut model, one_node_answered()?);
    let partly = draw(&model, 140, 40)?;
    assert!(partly.contains("1 of 3 nodes answered"), "{partly}");
    assert!(partly.contains("api"), "{partly}");
    assert!(model.loading.is_pending(Part::Status));

    // The last word: two nodes answered, one did not.
    let mut snapshot = model.snapshot.clone();
    snapshot.nodes[1].reachable = true;
    snapshot.nodes[2].problem = Some(
        "did not answer (ssh: connect to host 203.0.113.13 port 22: Connection timed out). Check that the host is up"
            .to_owned(),
    );
    apply(
        &mut model,
        Loaded::Status {
            snapshot: Box::new(snapshot),
            complete: true,
        },
    );
    assert!(!model.loading.is_pending(Part::Status));
    let services = draw(&model, 140, 40)?;
    assert!(services.contains("app-3 not answering"), "{services}");
    assert!(!services.contains("nodes answered"), "{services}");
    // A node that does not answer is an issue wherever issues are counted.
    assert!(model.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == "MM_UNREACHABLE"
            && diagnostic
                .message
                .starts_with("node 'app-3' did not answer")
    }),);
    model.loading = crate::presentation::tui::loading::Loading::default();
    model.tab = Tab::Overview;
    let overview = draw(&model, 140, 40)?;
    assert!(
        overview.contains("node 'app-3' did not answer"),
        "{overview}"
    );
    Ok(())
}

#[test]
fn a_cluster_lists_each_service_once_with_a_row_for_each_node()
-> Result<(), Box<dyn std::error::Error>> {
    let mut model = cluster_model()?;
    model.tab = Tab::Services;
    assert_eq!(
        model.selectable_rows(),
        5,
        "web, a row for each of its nodes, and api"
    );
    let rendered = draw(&model, 170, 48)?;
    for marker in [
        "▾ web  3 nodes",
        "├ app-1  web-1  healthy  Up 2 hours",
        "├ app-2  web-1  healthy  Up 2 hours",
        "└ app-3  no container",
        // One container on one node: the row names the node.
        "api  app-1",
        "app-3  ○ missing 0/1",
        "app-3: service 'web'",
    ] {
        assert!(
            rendered.contains(marker),
            "missing {marker:?} in:\n{rendered}"
        );
    }
    // A container that a newer one replaced is not a row.
    assert!(!rendered.contains("web-run-1f2e"), "{rendered}");
    model.collapsed.insert(("app".to_owned(), "web".to_owned()));
    let folded = draw(&model, 170, 48)?;
    assert!(folded.contains("▸ web  3 nodes"), "{folded}");
    assert!(!folded.contains("└ app-3"), "{folded}");

    // What differs on a node is an issue, and the release is on the Overview.
    model.tab = Tab::Overview;
    let overview = draw(&model, 170, 48)?;
    for marker in [
        "commit 3f2c9a1e5b7d",
        "node app-3: differs from its last deployment",
    ] {
        assert!(
            overview.contains(marker),
            "missing {marker:?} in:\n{overview}"
        );
    }
    Ok(())
}

/// The Services view counts the containers that are ready and says
/// `starting` and `no answer` where `mm service status` does.
#[test]
fn a_cluster_says_starting_and_no_answer_as_service_status_does()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::domain::runtime::ServiceState;

    let mut model = cluster_model()?;
    let mut snapshot = model.snapshot.clone();
    // web: ready on app-1, its health check still running on app-2, and
    // app-3 did not answer. api: its one container is still coming up.
    snapshot.nodes[2].reachable = false;
    snapshot.nodes[2].problem = Some("did not answer".to_owned());
    snapshot.nodes[2].drift.clear();
    let services = &mut snapshot.stacks[0].services;
    services[0].healthy_count = 1;
    services[0].containers[1].health = Some("starting".to_owned());
    services[0].containers[1].status = "Up 4 seconds (health: starting)".to_owned();
    services[0].nodes[1].state = ServiceState::Degraded;
    services[0].nodes[1].healthy_count = 0;
    services[0].nodes[2].state = ServiceState::Unknown;
    services[1].state = ServiceState::Degraded;
    services[1].healthy_count = 0;
    services[1].containers[0].health = Some("starting".to_owned());
    services[1].nodes[0].state = ServiceState::Degraded;
    services[1].nodes[0].healthy_count = 0;
    for service in services.iter_mut() {
        service.note_starting();
    }
    model.show_status(snapshot);
    model.tab = Tab::Services;
    let rendered = draw(&model, 170, 48)?;
    for marker in [
        // Two of three run, one is ready.
        "1/3",
        "app-2  web-1  starting  Up 4 seconds",
        "└ app-3  no answer",
        // The details of web, node by node.
        "starting 0/1",
        "unknown 0/1",
    ] {
        assert!(
            rendered.contains(marker),
            "missing {marker:?} in:\n{rendered}"
        );
    }
    assert!(!rendered.contains("2/3"), "{rendered}");
    assert!(!rendered.contains("no container"), "{rendered}");
    let web = rendered
        .lines()
        .find(|line| line.contains("web  3 nodes"))
        .ok_or("no row for web")?;
    assert!(web.contains("degraded"), "{web}");
    let api = rendered
        .lines()
        .find(|line| line.contains("api  app-1"))
        .ok_or("no row for api")?;
    assert!(api.contains("starting") && api.contains("0/1"), "{api}");
    assert!(!api.contains("degraded"), "{api}");

    model.tab = Tab::Environments;
    model.scroll = 2;
    let environments = draw(&model, 170, 48)?;
    for marker in ["web  degraded 1/3", "api  starting 0/1"] {
        assert!(
            environments.contains(marker),
            "missing {marker:?} in:\n{environments}"
        );
    }
    Ok(())
}

#[test]
fn a_cluster_on_screen_lists_what_each_node_runs_and_its_mesh()
-> Result<(), Box<dyn std::error::Error>> {
    let mut model = cluster_model()?;
    model.tab = Tab::Environments;
    model.scroll = 2;
    for (width, height) in [(140, 40), (170, 48)] {
        let rendered = draw(&model, width, height)?;
        for marker in [
            "● app-1  mmd 0.1.0  3f2c9a1e5b7d",
            "mesh 2/2 peers",
            "Nodes checked just now",
            "r check again",
        ] {
            assert!(
                rendered.contains(marker),
                "{width}x{height} lacks {marker:?} in:\n{rendered}"
            );
        }
    }
    // Before the check has answered, the live status says what it knows.
    model.environments.probes.insert(
        "production".to_owned(),
        crate::presentation::tui::environments::ProbeState::Checking,
    );
    let checking = draw(&model, 170, 48)?;
    assert!(
        checking.contains("● app-1  answering  3f2c9a1e5b7d"),
        "{checking}"
    );
    assert!(checking.contains("Checking the nodes"), "{checking}");
    Ok(())
}

#[test]
fn cluster_logs_name_node_and_container_and_mark_a_silent_node()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::presentation::tui::panel::{LogLine, LogsPanel, Panel, StreamState};

    let mut model = cluster_model()?;
    model.tab = Tab::Services;
    let target = model
        .selected_service_target()
        .ok_or("no service selected")?;
    let mut logs = LogsPanel::new(1, target);
    logs.state = StreamState::Streaming;
    for (node, text) in [("app-1", "GET / 200"), ("app-2", "GET /healthz 200")] {
        logs.push(LogLine {
            node: Some(node.to_owned()),
            container: Some("web-1".to_owned()),
            text: text.to_owned(),
            ..LogLine::default()
        });
    }
    logs.push(LogLine {
        node: Some("app-3".to_owned()),
        about_node: true,
        text: "did not answer: connection timed out".to_owned(),
        ..LogLine::default()
    });
    model.panel = Some(Panel::Logs(logs.clone()));
    let rendered = draw(&model, 140, 40)?;
    for marker in [
        "Logs · web  3 nodes, in order",
        "app-1 web-1 │ GET / 200",
        "app-2 web-1 │ GET /healthz 200",
        "▲ app-3       ! did not answer: connection timed out",
    ] {
        assert!(
            rendered.contains(marker),
            "missing {marker:?} in:\n{rendered}"
        );
    }
    // The filter finds a node's lines by its name.
    logs.filter = "app-2".to_owned();
    model.panel = Some(Panel::Logs(logs));
    let filtered = draw(&model, 140, 40)?;
    assert!(filtered.contains("1 of 3 lines"), "{filtered}");
    assert!(!filtered.contains("GET / 200"), "{filtered}");
    Ok(())
}

#[test]
fn the_deploy_review_shows_the_plan_and_each_node_while_it_applies()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::domain::rollout::{ProgressStep, RolloutProgress};
    use crate::presentation::tui::control::{StackOutcome, StartReview, apply_outcome};

    let mut model = cluster_model()?;
    let review = StartReview {
        plan_hash: "8a41c0de91f2a7b3".to_owned(),
        target: "prod-eu".to_owned(),
        stacks: vec!["app".to_owned()],
        routes: 2,
        remote: true,
        nodes: vec!["app-1".to_owned(), "app-2".to_owned(), "app-3".to_owned()],
        lines: vec![
            "Cluster deployment plan 8a41c0de91f2a7b3".to_owned(),
            "  moves:".to_owned(),
            "    app/worker: starts on app-2, then stops on app-1".to_owned(),
            "  rollout: app on app-1, then app-2, then app-3".to_owned(),
        ],
        rollback: None,
    };
    apply_outcome(&mut model, StackOutcome::Reviewed(Ok(review)));
    let rendered = draw(&model, 140, 40)?;
    for marker in [
        "Deploy to production?",
        "app on target 'prod-eu' (app-1, app-2, app-3)",
        "2 routes · plan 8a41c0de91f2",
        "app/worker: starts on app-2, then stops on app-1",
        "rollout: app on app-1, then app-2, then app-3",
        "enter deploy",
    ] {
        assert!(
            rendered.contains(marker),
            "missing {marker:?} in:\n{rendered}"
        );
    }
    if let Some(review) = model.review.as_mut() {
        review.stage = crate::presentation::tui::review::ReviewStage::Applying;
    }
    let step = |node: &str, step| StackOutcome::Progress(RolloutProgress::new(node, step));
    apply_outcome(&mut model, step("app-1", ProgressStep::Preparing));
    apply_outcome(
        &mut model,
        step("app-1", ProgressStep::Stack("app".to_owned())),
    );
    apply_outcome(
        &mut model,
        step("app-1", ProgressStep::Healthy("app".to_owned())),
    );
    apply_outcome(&mut model, step("app-2", ProgressStep::Preparing));
    let applying = draw(&model, 140, 40)?;
    for marker in [
        "Deploying to production",
        "app-1  waits for its next turn   stacks applied: app",
        "app-2  preparing",
        "app-3  waiting",
    ] {
        assert!(
            applying.contains(marker),
            "missing {marker:?} in:\n{applying}"
        );
    }
    apply_outcome(
        &mut model,
        StackOutcome::Started(Ok(
            "Applied 8a41c0de91f2 on 3 node(s) of prod-eu.".to_owned()
        )),
    );
    let applied = draw(&model, 140, 40)?;
    assert!(applied.contains("app-1  done"), "{applied}");
    assert!(applied.contains("Applied 8a41c0de91f2"), "{applied}");
    Ok(())
}

#[test]
fn only_keys_that_look_around_work_while_the_session_is_read()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::presentation::tui::loading::looks_around;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut model = reading_model()?;
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    for code in [
        KeyCode::Char('j'),
        KeyCode::Tab,
        KeyCode::Char('3'),
        KeyCode::Char('?'),
        KeyCode::Char('q'),
        KeyCode::Esc,
    ] {
        assert!(looks_around(&model, key(code)), "{code:?}");
    }
    for code in [
        KeyCode::Char('u'),
        KeyCode::Char('d'),
        KeyCode::Char('w'),
        KeyCode::Char('x'),
        KeyCode::Enter,
    ] {
        assert!(!looks_around(&model, key(code)), "{code:?}");
    }
    assert!(looks_around(
        &model,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    ));
    assert!(!looks_around(
        &model,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)
    ));
    // Another environment can be opened without waiting for this one.
    model.tab = Tab::Environments;
    assert!(looks_around(&model, key(KeyCode::Enter)));
    // The footer offers what works.
    let footer = draw(&model, 140, 40)?;
    assert!(
        footer.contains("switch") && !footer.contains("deploy"),
        "{footer}"
    );
    Ok(())
}

#[test]
fn a_remote_environments_tls_tab_shows_its_routes_not_this_machines_ca()
-> Result<(), Box<dyn std::error::Error>> {
    let mut model = remote_model()?;
    model.tab = Tab::Tls;
    let rendered = draw(&model, 170, 48)?;
    assert!(!rendered.contains("Local HTTPS"), "{rendered}");
    assert!(!rendered.contains("Local CA"), "{rendered}");
    assert!(!rendered.contains("guided setup"), "{rendered}");
    assert!(rendered.contains("HTTPS"), "{rendered}");
    Ok(())
}
