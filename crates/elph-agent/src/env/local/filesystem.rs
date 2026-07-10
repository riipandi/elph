//! Local filesystem implementation.

use super::LocalExecutionEnv;

use std::path::PathBuf;

use crate::session::id::create_tsid;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;

use crate::harness::types::{
    CreateDirOptions, CreateTempFileOptions, FileError, FileErrorCode, FileInfo, FileSystem, ReadTextLinesOptions,
    RemoveOptions, Result, err, ok,
};

impl FileSystem for LocalExecutionEnv {
    fn cwd(&self) -> &str {
        self.cwd
            .to_str()
            .expect("execution environment cwd must be valid UTF-8")
    }

    async fn absolute_path(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<String, FileError> {
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, None) {
            return aborted;
        }
        ok(Self::normalize_path(&self.resolve_path(path)))
    }

    async fn join_path(&self, parts: &[&str], abort_token: Option<&CancellationToken>) -> Result<String, FileError> {
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, None) {
            return aborted;
        }
        let mut joined = PathBuf::new();
        for part in parts {
            joined.push(part);
        }
        ok(Self::normalize_path(&joined))
    }

    async fn read_text_file(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<String, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, Some(&normalized)) {
            return aborted;
        }
        match fs::read_to_string(&resolved).await {
            Ok(content) => {
                if let Some(aborted) = Self::abort_file_error::<String>(abort_token, Some(&normalized)) {
                    return aborted;
                }
                ok(content)
            }
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn read_text_lines(
        &self,
        path: &str,
        options: Option<ReadTextLinesOptions>,
    ) -> Result<Vec<String>, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        let options = options.unwrap_or(ReadTextLinesOptions {
            max_lines: None,
            abort_token: None,
        });
        if let Some(aborted) = Self::abort_file_error::<Vec<String>>(options.abort_token.as_ref(), Some(&normalized)) {
            return aborted;
        }
        if options.max_lines == Some(0) {
            return ok(Vec::new());
        }

        let file = match fs::File::open(&resolved).await {
            Ok(file) => file,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        };
        let mut reader = BufReader::new(file).lines();
        let mut lines = Vec::new();
        while let Some(line) = match reader.next_line().await {
            Ok(line) => line,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        } {
            if let Some(aborted) =
                Self::abort_file_error::<Vec<String>>(options.abort_token.as_ref(), Some(&normalized))
            {
                return aborted;
            }
            lines.push(line);
            if options.max_lines.is_some_and(|max| lines.len() >= max) {
                break;
            }
        }
        if let Some(aborted) = Self::abort_file_error::<Vec<String>>(options.abort_token.as_ref(), Some(&normalized)) {
            return aborted;
        }
        ok(lines)
    }

    async fn read_binary_file(
        &self,
        path: &str,
        abort_token: Option<&CancellationToken>,
    ) -> Result<Vec<u8>, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<Vec<u8>>(abort_token, Some(&normalized)) {
            return aborted;
        }
        match fs::read(&resolved).await {
            Ok(content) => {
                if let Some(aborted) = Self::abort_file_error::<Vec<u8>>(abort_token, Some(&normalized)) {
                    return aborted;
                }
                ok(content)
            }
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn write_file(
        &self,
        path: &str,
        content: &[u8],
        abort_token: Option<&CancellationToken>,
    ) -> Result<(), FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<()>(abort_token, Some(&normalized)) {
            return aborted;
        }
        if let Some(parent) = resolved.parent() {
            if let Err(error) = fs::create_dir_all(parent).await {
                return err(Self::to_file_error(error, Some(&Self::normalize_path(parent))));
            }
            if let Some(aborted) = Self::abort_file_error::<()>(abort_token, Some(&normalized)) {
                return aborted;
            }
        }
        match fs::write(&resolved, content).await {
            Ok(()) => ok(()),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn append_file(
        &self,
        path: &str,
        content: &[u8],
        abort_token: Option<&CancellationToken>,
    ) -> Result<(), FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<()>(abort_token, Some(&normalized)) {
            return aborted;
        }
        if let Some(parent) = resolved.parent()
            && let Err(error) = fs::create_dir_all(parent).await
        {
            return err(Self::to_file_error(error, Some(&Self::normalize_path(parent))));
        }
        use tokio::io::AsyncWriteExt;
        let mut file = match fs::OpenOptions::new().create(true).append(true).open(&resolved).await {
            Ok(file) => file,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        };
        match file.write_all(content).await {
            Ok(()) => ok(()),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn file_info(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<FileInfo, FileError> {
        if let Some(aborted) = Self::abort_file_error::<FileInfo>(abort_token, None) {
            return aborted;
        }
        let resolved = self.resolve_path(path);
        self.metadata_for_path(&resolved).await
    }

    async fn list_dir(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<Vec<FileInfo>, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<Vec<FileInfo>>(abort_token, Some(&normalized)) {
            return aborted;
        }
        let mut entries = match fs::read_dir(&resolved).await {
            Ok(entries) => entries,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        };

        let mut result = Vec::new();
        while let Some(entry) = match entries.next_entry().await {
            Ok(entry) => entry,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        } {
            if let Some(aborted) = Self::abort_file_error::<Vec<FileInfo>>(abort_token, Some(&normalized)) {
                return aborted;
            }
            let entry_path = entry.path();
            match self.metadata_for_path(&entry_path).await {
                Result::Ok(info) => result.push(info),
                Result::Err(error) => return err(error),
            }
        }
        ok(result)
    }

    async fn canonical_path(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<String, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, Some(&normalized)) {
            return aborted;
        }
        match fs::canonicalize(&resolved).await {
            Ok(canonical) => ok(Self::normalize_path(&canonical)),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn exists(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<bool, FileError> {
        let info = self.file_info(path, abort_token).await;
        match info {
            Result::Ok(_) => ok(true),
            Result::Err(error) if error.code == FileErrorCode::NotFound => ok(false),
            Result::Err(error) => err(error),
        }
    }

    async fn create_dir(&self, path: &str, options: Option<CreateDirOptions>) -> Result<(), FileError> {
        let options = options.unwrap_or_default();
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<()>(options.abort_token.as_ref(), Some(&normalized)) {
            return aborted;
        }
        let result = if options.recursive {
            fs::create_dir_all(&resolved).await
        } else {
            fs::create_dir(&resolved).await
        };
        match result {
            Ok(()) => ok(()),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn remove(&self, path: &str, options: Option<RemoveOptions>) -> Result<(), FileError> {
        let options = options.unwrap_or_default();
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<()>(options.abort_token.as_ref(), Some(&normalized)) {
            return aborted;
        }

        let metadata = fs::symlink_metadata(&resolved).await;
        if metadata.is_err() {
            if options.force {
                return ok(());
            }
            return err(Self::to_file_error(
                metadata.expect_err("checked error"),
                Some(&normalized),
            ));
        }

        let metadata = metadata.expect("metadata exists");
        let result = if metadata.is_dir() {
            if options.recursive {
                fs::remove_dir_all(&resolved).await
            } else {
                fs::remove_dir(&resolved).await
            }
        } else {
            fs::remove_file(&resolved).await
        };
        match result {
            Ok(()) => ok(()),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn create_temp_dir(
        &self,
        prefix: &str,
        abort_token: Option<&CancellationToken>,
    ) -> Result<String, FileError> {
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, None) {
            return aborted;
        }
        let base = std::env::temp_dir();
        let path = base.join(format!("{prefix}{}", create_tsid()));
        match fs::create_dir_all(&path).await {
            Ok(()) => ok(Self::normalize_path(&path)),
            Err(error) => err(Self::to_file_error(error, None)),
        }
    }

    async fn create_temp_file(&self, options: Option<CreateTempFileOptions>) -> Result<String, FileError> {
        let options = options.unwrap_or_default();
        if let Some(aborted) = Self::abort_file_error::<String>(options.abort_token.as_ref(), None) {
            return aborted;
        }
        let dir = match self.create_temp_dir("tmp-", options.abort_token.as_ref()).await {
            Result::Ok(dir_path) => dir_path,
            Result::Err(error) => return err(error),
        };
        let dir_path = dir;
        let file_path = PathBuf::from(&dir_path).join(format!("{}{}{}", options.prefix, create_tsid(), options.suffix));
        let normalized = Self::normalize_path(&file_path);
        match fs::write(&file_path, &[] as &[u8]).await {
            Ok(()) => ok(normalized),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn cleanup(&self) {}
}
