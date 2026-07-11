//! Integration tests for the tuie [`ElphShellHost`] bridge.

use std::sync::{Arc, Mutex};

use elph::platform::{self, Paths};
use elph::shell::{ElphApp, ElphShellHost};
use elph_tui::{PromptAction, ShellHost, TranscriptRole};

async fn bootstrap_test_app(tmp: &tempfile::TempDir) -> Arc<Mutex<ElphApp>> {
    let home = tmp.path().join("home");
    let data = tmp.path().join("data");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&home).expect("home dir");
    std::fs::create_dir_all(&project).expect("project dir");

    // SAFETY: single-threaded test runtime; vars scoped to this test process.
    unsafe {
        std::env::set_var("ELPH_HOME", &home);
        std::env::set_var("ELPH_DATA_DIR", &data);
        std::env::set_var("ELPH_PROJECT_DIR", &project);
    }

    let paths = Paths::from_dirs(home, data, project);
    platform::bootstrap::ensure_with_paths(&paths, "test")
        .await
        .expect("bootstrap home");
    let settings = platform::Settings::load(&paths).expect("load settings");
    let app = ElphApp::bootstrap(settings, None).await.expect("bootstrap app");
    Arc::new(Mutex::new(app))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chrome_reflects_session_metadata() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let app = bootstrap_test_app(&tmp).await;
    let host = ElphShellHost::new(Arc::clone(&app));

    let chrome = host.chrome();
    let app = app.lock().expect("lock");
    assert_eq!(chrome.session_id, app.session_id);
    assert_eq!(chrome.turn, app.turn);
    assert_eq!(chrome.model_name, app.prompt.model_name);
    assert_eq!(chrome.mode, app.prompt.mode);
    assert!(!chrome.sidebar_open);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompt_text_reads_backend_draft() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let app = bootstrap_test_app(&tmp).await;
    app.lock().expect("lock").prompt.set_value("backend draft");

    let host = ElphShellHost::new(app);
    assert_eq!(host.prompt_text(), "backend draft");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn on_prompt_action_queues_while_running() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let app = bootstrap_test_app(&tmp).await;
    {
        let mut guard = app.lock().expect("lock");
        guard.agent_running = true;
    }

    let mut host = ElphShellHost::new(Arc::clone(&app));
    host.on_prompt_action(PromptAction::Queue("next message".into()));

    let guard = app.lock().expect("lock");
    assert_eq!(guard.prompt_queue.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transcript_lines_include_user_entries() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let app = bootstrap_test_app(&tmp).await;
    {
        let mut guard = app.lock().expect("lock");
        guard.chat.entries.push(elph_tui::TranscriptEntry::user("hello shell"));
    }

    let host = ElphShellHost::new(app);
    let lines = host.transcript_lines();
    assert!(lines.iter().any(|l| l.contains("hello shell")));
    assert!(lines.iter().any(|l| l.starts_with('›')));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slash_commands_exposed_to_palette() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let app = bootstrap_test_app(&tmp).await;
    let host = ElphShellHost::new(app);
    let commands = host.commands();
    assert!(!commands.is_empty());
    assert!(commands.iter().any(|cmd| cmd.name == "help"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clear_via_host_empties_backend_prompt() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let app = bootstrap_test_app(&tmp).await;
    app.lock().expect("lock").prompt.set_value("draft");

    let mut host = ElphShellHost::new(Arc::clone(&app));
    host.on_prompt_action(PromptAction::Clear);
    assert!(app.lock().expect("lock").prompt.value().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slash_submit_via_host_writes_system_line() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let app = bootstrap_test_app(&tmp).await;
    let mut host = ElphShellHost::new(Arc::clone(&app));
    host.on_prompt_action(PromptAction::Submit("/status".into()));

    let guard = app.lock().expect("lock");
    assert!(guard.prompt.value().is_empty());
    assert!(
        guard
            .chat
            .entries
            .iter()
            .any(|entry| { entry.role == TranscriptRole::System && entry.content.contains("status") })
    );
}
