//! Tests for `LocalExecutionEnv` — ported from pi-agent `test/harness/nodejs-env.test.ts`.

use elph_agent::env::LocalExecutionEnv;
use elph_agent::harness::types::{
    CreateDirOptions, FileErrorCode, FileKind, FileSystem, Result, Shell, ShellExecOptions, get_or_throw,
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn env_in_temp() -> (TempDir, LocalExecutionEnv) {
    let temp = TempDir::new().expect("temp dir");
    let env = LocalExecutionEnv::new(temp.path());
    (temp, env)
}

#[tokio::test]
async fn reads_writes_lists_and_removes_files() {
    let (_temp, env) = env_in_temp();
    let root = env.cwd().to_string();

    get_or_throw(env.create_dir("nested/child", true).await);
    get_or_throw(env.write_file("nested/child/file.txt", "hel").await);
    get_or_throw(FileSystem::append_file(&env, "nested/child/file.txt", b"lo", None).await);
    assert_eq!(
        get_or_throw(env.read_text_file("nested/child/file.txt", None).await),
        "hello"
    );
    assert_eq!(
        get_or_throw(
            env.read_text_lines(
                "nested/child/file.txt",
                Some(elph_agent::harness::types::ReadTextLinesOptions {
                    max_lines: Some(1),
                    abort_token: None,
                }),
            )
            .await
        ),
        vec!["hello".to_string()]
    );
    assert_eq!(
        get_or_throw(env.read_binary_file("nested/child/file.txt", None).await),
        b"hello".to_vec()
    );

    let entries = get_or_throw(env.list_dir("nested/child", None).await);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "file.txt");
    assert_eq!(entries[0].path, format!("{root}/nested/child/file.txt"));
    assert_eq!(entries[0].kind, FileKind::File);
    assert_eq!(entries[0].size, 5);

    assert!(get_or_throw(env.exists("nested/child/file.txt", None).await));
    get_or_throw(env.remove("nested/child/file.txt", None).await);
    assert!(!get_or_throw(env.exists("nested/child/file.txt", None).await));
}

#[tokio::test]
async fn absolute_path_and_join_path() {
    let (_temp, env) = env_in_temp();
    let root = env.cwd();

    assert_eq!(
        get_or_throw(env.absolute_path("nested/child", None).await),
        format!("{root}/nested/child")
    );
    assert_eq!(
        get_or_throw(env.join_path(&[root, "nested", "child"], None).await),
        format!("{root}/nested/child")
    );
}

#[tokio::test]
async fn returns_file_error_for_missing_paths() {
    let (_temp, env) = env_in_temp();
    let root = env.cwd().to_string();

    let info = env.file_info("missing.txt", None).await;
    assert!(info.is_err());
    if let Result::Err(error) = info {
        assert_eq!(error.code, FileErrorCode::NotFound);
        assert_eq!(error.path.as_deref(), Some(format!("{root}/missing.txt").as_str()));
    }

    assert!(!get_or_throw(env.exists("missing.txt", None).await));
}

#[tokio::test]
async fn returns_file_error_for_listing_non_directories() {
    let (_temp, env) = env_in_temp();
    get_or_throw(env.write_file("file.txt", "hello").await);
    let result = env.list_dir("file.txt", None).await;
    assert!(result.is_err());
    if let Result::Err(error) = result {
        assert_eq!(error.code, FileErrorCode::NotDirectory);
    }
}

#[tokio::test]
async fn appends_to_new_files_and_creates_parent_directories() {
    let (_temp, env) = env_in_temp();
    get_or_throw(FileSystem::append_file(&env, "new/nested/file.txt", b"a", None).await);
    get_or_throw(FileSystem::append_file(&env, "new/nested/file.txt", b"b", None).await);
    assert_eq!(
        get_or_throw(env.read_text_file("new/nested/file.txt", None).await),
        "ab"
    );
}

#[tokio::test]
async fn creates_temporary_directories_and_files() {
    let (_temp, env) = env_in_temp();
    let temp_dir = get_or_throw(env.create_temp_dir("node-env-test-", None).await);
    assert!(std::path::Path::new(&temp_dir).exists());

    let temp_file = get_or_throw(
        env.create_temp_file(Some(elph_agent::harness::types::CreateTempFileOptions {
            prefix: "prefix-".to_string(),
            suffix: ".txt".to_string(),
            abort_token: None,
        }))
        .await,
    );
    assert!(std::path::Path::new(&temp_file).exists());
    assert!(temp_file.ends_with(".txt"));
}

#[tokio::test]
async fn honors_create_dir_recursive_false() {
    let (_temp, env) = env_in_temp();
    let create_result = FileSystem::create_dir(
        &env,
        "missing/child",
        Some(CreateDirOptions {
            recursive: false,
            abort_token: None,
        }),
    )
    .await;
    assert!(create_result.is_err());
    if let Result::Err(error) = create_result {
        assert_eq!(error.code, FileErrorCode::NotFound);
    }
}

#[tokio::test]
async fn returns_aborted_results_for_cancelled_file_operations() {
    let (_temp, env) = env_in_temp();
    get_or_throw(env.write_file("file.txt", "hello").await);
    let token = CancellationToken::new();
    token.cancel();

    fn assert_aborted<T>(result: Result<T, elph_agent::harness::types::FileError>) {
        assert!(result.is_err());
        if let Result::Err(error) = result {
            assert_eq!(error.code, FileErrorCode::Aborted);
        }
    }

    assert_aborted(env.read_text_file("file.txt", Some(&token)).await);
    assert_aborted(
        env.read_text_lines(
            "file.txt",
            Some(elph_agent::harness::types::ReadTextLinesOptions {
                max_lines: None,
                abort_token: Some(token.clone()),
            }),
        )
        .await,
    );
    assert_aborted(env.read_binary_file("file.txt", Some(&token)).await);
    assert_aborted(FileSystem::write_file(&env, "other.txt", b"hello", Some(&token)).await);
    assert_aborted(env.list_dir(".", Some(&token)).await);
}

#[tokio::test]
async fn cleanup_is_best_effort() {
    let (_temp, env) = env_in_temp();
    FileSystem::cleanup(&env).await;
}

#[tokio::test]
async fn executes_commands_in_cwd_with_env_overrides() {
    let (_temp, env) = env_in_temp();
    let root = std::fs::canonicalize(env.cwd()).expect("canonical cwd");

    let result = get_or_throw(
        env.exec(
            "printf '%s:%s' \"$PWD\" \"$NODE_ENV_TEST\"",
            Some(ShellExecOptions {
                cwd: None,
                env: Some([("NODE_ENV_TEST".to_string(), "ok".to_string())].into()),
                timeout: None,
                abort_token: None,
                on_stdout: None,
                on_stderr: None,
            }),
        )
        .await,
    );

    let expected_root = root.to_string_lossy().replace('\\', "/");
    let actual = result.stdout.trim_end_matches('\n');
    assert_eq!(actual, format!("{expected_root}:ok"));
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn streams_stdout_and_stderr_chunks() {
    let (_temp, env) = env_in_temp();
    let stdout = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let stderr = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let stdout_capture = stdout.clone();
    let stderr_capture = stderr.clone();

    let result = get_or_throw(
        env.exec(
            "printf out; printf err 1>&2",
            Some(ShellExecOptions {
                cwd: None,
                env: None,
                timeout: None,
                abort_token: None,
                on_stdout: Some(std::sync::Arc::new(move |chunk| {
                    stdout_capture.lock().expect("lock").push_str(chunk);
                })),
                on_stderr: Some(std::sync::Arc::new(move |chunk| {
                    stderr_capture.lock().expect("lock").push_str(chunk);
                })),
            }),
        )
        .await,
    );

    assert!(result.stdout.contains("out"));
    assert!(result.stderr.contains("err"));
    assert!(stdout.lock().expect("lock").contains("out"));
    assert!(stderr.lock().expect("lock").contains("err"));
}
