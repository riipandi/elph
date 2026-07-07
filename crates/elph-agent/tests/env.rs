//! Tests for `LocalExecutionEnv`.

use elph_agent::env::LocalExecutionEnv;
use elph_agent::harness::types::{
    CreateDirOptions, ExecutionErrorCode, FileErrorCode, FileKind, FileSystem, ReadTextLinesOptions, RemoveOptions,
    Result, Shell, ShellExecOptions, get_or_throw,
};
use elph_agent::harness::utils::execute_shell_with_capture;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

#[cfg(unix)]
use std::os::unix::fs::symlink;

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
async fn honors_create_dir_recursive_false_and_remove_options() {
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

    get_or_throw(env.write_file("dir/child/file.txt", "hello").await);
    let remove_directory = env
        .remove(
            "dir",
            Some(RemoveOptions {
                recursive: false,
                force: false,
                abort_token: None,
            }),
        )
        .await;
    assert!(remove_directory.is_err());
    get_or_throw(
        env.remove(
            "dir",
            Some(RemoveOptions {
                recursive: true,
                force: false,
                abort_token: None,
            }),
        )
        .await,
    );
    assert!(!get_or_throw(env.exists("dir", None).await));

    let remove_missing = env
        .remove(
            "missing",
            Some(RemoveOptions {
                recursive: false,
                force: false,
                abort_token: None,
            }),
        )
        .await;
    assert!(remove_missing.is_err());
    get_or_throw(
        env.remove(
            "missing",
            Some(RemoveOptions {
                recursive: false,
                force: true,
                abort_token: None,
            }),
        )
        .await,
    );
}

#[cfg(unix)]
#[tokio::test]
async fn returns_file_info_for_files_directories_and_symlinks_without_following_symlinks() {
    let (_temp, env) = env_in_temp();
    let root = env.cwd().to_string();

    get_or_throw(env.create_dir("dir", true).await);
    get_or_throw(env.write_file("dir/file.txt", "hello").await);
    symlink(format!("{root}/dir/file.txt"), format!("{root}/file-link")).expect("file symlink");
    symlink(format!("{root}/dir"), format!("{root}/dir-link")).expect("dir symlink");

    let dir_info = get_or_throw(env.file_info("dir", None).await);
    assert_eq!(dir_info.name, "dir");
    assert_eq!(dir_info.path, format!("{root}/dir"));
    assert_eq!(dir_info.kind, FileKind::Directory);

    let file_info = get_or_throw(env.file_info("dir/file.txt", None).await);
    assert_eq!(file_info.name, "file.txt");
    assert_eq!(file_info.path, format!("{root}/dir/file.txt"));
    assert_eq!(file_info.kind, FileKind::File);
    assert_eq!(file_info.size, 5);

    let file_link_info = get_or_throw(env.file_info("file-link", None).await);
    assert_eq!(file_link_info.name, "file-link");
    assert_eq!(file_link_info.path, format!("{root}/file-link"));
    assert_eq!(file_link_info.kind, FileKind::Symlink);

    let dir_link_info = get_or_throw(env.file_info("dir-link", None).await);
    assert_eq!(dir_link_info.name, "dir-link");
    assert_eq!(dir_link_info.path, format!("{root}/dir-link"));
    assert_eq!(dir_link_info.kind, FileKind::Symlink);

    let canonical = get_or_throw(env.canonical_path("file-link", None).await);
    let expected = std::fs::canonicalize(format!("{root}/dir/file.txt")).expect("canonical target");
    assert_eq!(canonical, expected.to_string_lossy().replace('\\', "/"));
}

#[cfg(unix)]
#[tokio::test]
async fn lists_symlinks_as_symlinks() {
    let (_temp, env) = env_in_temp();
    let root = env.cwd().to_string();

    get_or_throw(env.write_file("target.txt", "hello").await);
    symlink(format!("{root}/target.txt"), format!("{root}/link.txt")).expect("symlink");

    let mut entries = get_or_throw(env.list_dir(".", None).await);
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "link.txt");
    assert_eq!(entries[0].kind, FileKind::Symlink);
    assert_eq!(entries[1].name, "target.txt");
    assert_eq!(entries[1].kind, FileKind::File);
}

#[tokio::test]
async fn stops_reading_text_lines_at_requested_limit() {
    let (_temp, env) = env_in_temp();
    get_or_throw(env.write_file("file.txt", "one\ntwo\nthree").await);
    assert_eq!(
        get_or_throw(
            env.read_text_lines(
                "file.txt",
                Some(ReadTextLinesOptions {
                    max_lines: Some(1),
                    abort_token: None,
                }),
            )
            .await
        ),
        vec!["one".to_string()]
    );
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

#[tokio::test]
async fn returns_non_zero_command_exit_codes_as_successful_execution_results() {
    let (_temp, env) = env_in_temp();
    let result = get_or_throw(env.exec("exit 7", None).await);
    assert_eq!(result.stdout, "");
    assert_eq!(result.stderr, "");
    assert_eq!(result.exit_code, 7);
}

#[tokio::test]
async fn returns_timeout_errors_for_commands_exceeding_timeout() {
    let (_temp, env) = env_in_temp();
    let result = env
        .exec(
            "sleep 5",
            Some(ShellExecOptions {
                cwd: None,
                env: None,
                timeout: Some(1),
                abort_token: None,
                on_stdout: None,
                on_stderr: None,
            }),
        )
        .await;
    assert!(result.is_err());
    if let Result::Err(error) = result {
        assert_eq!(error.code, ExecutionErrorCode::Timeout);
    }
}

#[tokio::test]
async fn returns_callback_errors_from_exec_stream_handlers() {
    let (_temp, env) = env_in_temp();
    let result = env
        .exec(
            "printf out",
            Some(ShellExecOptions {
                cwd: None,
                env: None,
                timeout: None,
                abort_token: None,
                on_stdout: Some(std::sync::Arc::new(|_| {
                    panic!("callback failed");
                })),
                on_stderr: None,
            }),
        )
        .await;
    assert!(result.is_err());
    if let Result::Err(error) = result {
        assert_eq!(error.code, ExecutionErrorCode::CallbackError);
        assert_eq!(error.message, "callback failed");
    }
}

#[tokio::test]
async fn returns_shell_unavailable_and_spawn_errors() {
    let (temp, env) = env_in_temp();
    let root = temp.path().to_string_lossy().to_string();

    let missing_shell_env = LocalExecutionEnv::new(temp.path()).with_shell_path(format!("{root}/missing-shell"));
    let missing_shell = missing_shell_env.exec("printf ok", None).await;
    assert!(missing_shell.is_err());
    if let Result::Err(error) = missing_shell {
        assert_eq!(error.code, ExecutionErrorCode::ShellUnavailable);
    }

    let shell_path = format!("{root}/not-executable-shell");
    get_or_throw(env.write_file("not-executable-shell", "not executable").await);
    let spawn_error_env = LocalExecutionEnv::new(temp.path()).with_shell_path(shell_path);
    let spawn_error = spawn_error_env.exec("printf ok", None).await;
    assert!(spawn_error.is_err());
    if let Result::Err(error) = spawn_error {
        assert_eq!(error.code, ExecutionErrorCode::SpawnError);
    }
}

#[tokio::test]
async fn returns_aborted_result_for_aborted_commands() {
    let (_temp, env) = env_in_temp();
    let token = CancellationToken::new();
    let exec_future = env.exec(
        "sleep 5",
        Some(ShellExecOptions {
            cwd: None,
            env: None,
            timeout: None,
            abort_token: Some(token.clone()),
            on_stdout: None,
            on_stderr: None,
        }),
    );
    token.cancel();
    let result = exec_future.await;
    assert!(result.is_err());
    if let Result::Err(error) = result {
        assert_eq!(error.code, ExecutionErrorCode::Aborted);
    }
}

#[tokio::test]
async fn captures_large_shell_output_to_full_output_file() {
    let (_temp, env) = env_in_temp();
    let result = get_or_throw(execute_shell_with_capture(&env, "yes line | head -n 15000", None).await);
    assert!(result.truncated);
    let full_output_path = result
        .full_output_path
        .expect("full output path should be set for truncated capture");
    let full_output = get_or_throw(env.read_text_file(&full_output_path, None).await);
    assert!(full_output.split('\n').count() > 10_000);
    assert!(result.output.len() < full_output.len());
}
