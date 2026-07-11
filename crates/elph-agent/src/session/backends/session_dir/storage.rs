//! Multi-file session directory storage.

use std::path::{Path, PathBuf};

use serde_json::json;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;

use crate::session::id::generate_entry_id;
use crate::session::storage_utils::{append_to_index, build_index, create_leaf_entry, find_entries, get_path_to_root};
use crate::session::types::{
    SessionDirMetadata, SessionError, SessionErrorCode, SessionIndex, SessionStorage, SessionTreeEntry,
};
use crate::types::AgentMessage;

use super::chat::{tree_message_to_chat_line, user_prompt_text};
use super::layout::{
    CHAT_HISTORY_FILE, EVENTS_FILE, PROMPT_CONTEXT_FILE, PROMPT_CONTEXT_VERSION, PROMPT_HISTORY_FILE, SESSION_SUBDIRS,
    SUMMARY_FILE, SYSTEM_PROMPT_FILE, UPDATES_FILE,
};
use super::summary::SessionSummary;

#[derive(Clone)]
pub struct SessionDirStorage {
    session_dir: PathBuf,
    metadata: SessionDirMetadata,
    index: SessionIndex,
}

impl SessionDirStorage {
    pub async fn open(session_dir: impl AsRef<Path>) -> Result<Self, SessionError> {
        let session_dir = session_dir.as_ref().to_path_buf();
        let summary = read_summary(&session_dir).await?;
        let metadata = summary_to_metadata(&summary, &session_dir);
        let entries = load_events(&session_dir).await?;
        let leaf_id = entries
            .iter()
            .rev()
            .find_map(crate::session::storage_utils::leaf_id_after_entry);
        let index = build_index(entries, leaf_id)?;
        Ok(Self {
            session_dir,
            metadata,
            index,
        })
    }

    pub async fn create(session_dir: impl AsRef<Path>, options: SessionDirCreateOptions) -> Result<Self, SessionError> {
        let session_dir = session_dir.as_ref().to_path_buf();
        fs::create_dir_all(&session_dir)
            .await
            .map_err(|error| storage_error(&session_dir, format!("failed to create session dir: {error}")))?;

        for subdir in SESSION_SUBDIRS {
            fs::create_dir_all(session_dir.join(subdir))
                .await
                .map_err(|error| storage_error(&session_dir, format!("failed to create {subdir}: {error}")))?;
        }

        let created_at = crate::messages::now_iso_timestamp();
        let summary = SessionSummary::new(
            options.session_id.clone(),
            options.cwd.clone(),
            created_at.clone(),
            options.parent_session_id.clone(),
        );
        write_summary(&session_dir, &summary).await?;

        write_text_file(&session_dir, SYSTEM_PROMPT_FILE, options.system_prompt.as_deref().unwrap_or("")).await?;
        write_json_file(
            &session_dir,
            PROMPT_CONTEXT_FILE,
            &json!({
                "version": PROMPT_CONTEXT_VERSION,
                "prompt_mode": "extend",
                "system_prompt": "elph",
            }),
        )
        .await?;

        for file in [CHAT_HISTORY_FILE, EVENTS_FILE, UPDATES_FILE, PROMPT_HISTORY_FILE] {
            write_text_file(&session_dir, file, "").await?;
        }

        let metadata = summary_to_metadata(&summary, &session_dir);
        Ok(Self {
            session_dir,
            metadata,
            index: build_index(Vec::new(), None)?,
        })
    }

    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    async fn append_event(&self, entry: &SessionTreeEntry) -> Result<(), SessionError> {
        append_jsonl_line(&self.session_dir, EVENTS_FILE, entry).await
    }

    async fn append_chat_mirror(&self, message: &AgentMessage) -> Result<(), SessionError> {
        let Some(line) = tree_message_to_chat_line(message) else {
            return Ok(());
        };
        append_jsonl_value(&self.session_dir, CHAT_HISTORY_FILE, &line).await
    }

    async fn append_prompt_history(&self, message: &AgentMessage) -> Result<(), SessionError> {
        let Some(text) = user_prompt_text(message) else {
            return Ok(());
        };
        let line = json!({
            "timestamp": crate::messages::now_iso_timestamp(),
            "prompt": text,
        });
        append_jsonl_value(&self.session_dir, PROMPT_HISTORY_FILE, &line).await
    }

    async fn touch_summary(&self) -> Result<(), SessionError> {
        let mut summary = read_summary(&self.session_dir).await?;
        summary.touch(crate::messages::now_iso_timestamp());
        write_summary(&self.session_dir, &summary).await
    }
}

#[derive(Debug, Clone)]
pub struct SessionDirCreateOptions {
    pub cwd: String,
    pub session_id: String,
    pub parent_session_id: Option<String>,
    pub system_prompt: Option<String>,
}

fn summary_to_metadata(summary: &SessionSummary, session_dir: &Path) -> SessionDirMetadata {
    SessionDirMetadata {
        id: summary.info.id.clone(),
        created_at: summary.created_at.clone(),
        cwd: summary.info.cwd.clone(),
        dir: session_dir.to_string_lossy().to_string(),
        parent_session_id: summary.parent_session_id.clone(),
    }
}

pub async fn load_session_metadata(session_dir: impl AsRef<Path>) -> Result<SessionDirMetadata, SessionError> {
    let session_dir = session_dir.as_ref();
    let summary = read_summary(session_dir).await?;
    Ok(summary_to_metadata(&summary, session_dir))
}

async fn read_summary(session_dir: &Path) -> Result<SessionSummary, SessionError> {
    let path = session_dir.join(SUMMARY_FILE);
    let content = fs::read_to_string(&path)
        .await
        .map_err(|error| storage_error(session_dir, format!("failed to read {}: {error}", SUMMARY_FILE)))?;
    serde_json::from_str(&content)
        .map_err(|error| invalid_session(session_dir, format!("invalid {SUMMARY_FILE}: {error}")))
}

async fn write_summary(session_dir: &Path, summary: &SessionSummary) -> Result<(), SessionError> {
    write_json_file(session_dir, SUMMARY_FILE, summary).await
}

async fn load_events(session_dir: &Path) -> Result<Vec<SessionTreeEntry>, SessionError> {
    let path = session_dir.join(EVENTS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)
        .await
        .map_err(|error| storage_error(session_dir, format!("failed to read {EVENTS_FILE}: {error}")))?;
    let mut entries = Vec::new();
    for (index, line) in content.lines().filter(|line| !line.trim().is_empty()).enumerate() {
        entries.push(parse_event_line(line, session_dir, index + 1)?);
    }
    Ok(entries)
}

fn parse_event_line(line: &str, session_dir: &Path, line_number: usize) -> Result<SessionTreeEntry, SessionError> {
    let parsed: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| invalid_entry(session_dir, line_number, format!("is not valid JSON: {error}")))?;
    let entry_type = parsed
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_entry(session_dir, line_number, "is missing entry type"))?;
    let id = parsed
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_entry(session_dir, line_number, "is missing entry id"))?;
    let parent_id = parsed.get("parentId");
    if parent_id.is_some() && !parent_id.is_none_or(|value| value.is_null() || value.is_string()) {
        return Err(invalid_entry(session_dir, line_number, "has invalid parentId"));
    }
    let timestamp = parsed
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_entry(session_dir, line_number, "is missing timestamp"))?;
    if entry_type == "leaf" {
        let target_id = parsed.get("targetId");
        if target_id.is_some() && !target_id.is_none_or(|value| value.is_null() || value.is_string()) {
            return Err(invalid_entry(session_dir, line_number, "has invalid targetId"));
        }
    }
    let _ = (id, timestamp);
    serde_json::from_value(parsed)
        .map_err(|error| invalid_entry(session_dir, line_number, format!("is not a valid session entry: {error}")))
}

async fn append_jsonl_line(session_dir: &Path, file: &str, entry: &SessionTreeEntry) -> Result<(), SessionError> {
    let line = serde_json::to_string(entry)
        .map_err(|error| storage_error(session_dir, format!("failed to encode entry: {error}")))?;
    append_line(session_dir, file, &line).await
}

async fn append_jsonl_value(session_dir: &Path, file: &str, value: &serde_json::Value) -> Result<(), SessionError> {
    let line = serde_json::to_string(value)
        .map_err(|error| storage_error(session_dir, format!("failed to encode value: {error}")))?;
    append_line(session_dir, file, &line).await
}

async fn append_line(session_dir: &Path, file_name: &str, line: &str) -> Result<(), SessionError> {
    let path = session_dir.join(file_name);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .map_err(|error| storage_error(session_dir, format!("failed to open {file_name}: {error}")))?;
    file.write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|error| storage_error(session_dir, format!("failed to append {file_name}: {error}")))?;
    file.flush()
        .await
        .map_err(|error| storage_error(session_dir, format!("failed to flush {file_name}: {error}")))?;
    Ok(())
}

async fn write_text_file(session_dir: &Path, file: &str, content: &str) -> Result<(), SessionError> {
    let path = session_dir.join(file);
    fs::write(&path, content)
        .await
        .map_err(|error| storage_error(session_dir, format!("failed to write {file}: {error}")))?;
    Ok(())
}

async fn write_json_file(session_dir: &Path, file: &str, value: &impl serde::Serialize) -> Result<(), SessionError> {
    let line = serde_json::to_string_pretty(value)
        .map_err(|error| storage_error(session_dir, format!("failed to encode {file}: {error}")))?;
    write_text_file(session_dir, file, &format!("{line}\n")).await
}

fn storage_error(path: &Path, message: impl Into<String>) -> SessionError {
    SessionError::new(
        SessionErrorCode::Storage,
        format!("session store {}: {}", path.display(), message.into()),
    )
}

fn invalid_session(path: &Path, message: impl Into<String>) -> SessionError {
    SessionError::new(
        SessionErrorCode::InvalidSession,
        format!("session store {}: {}", path.display(), message.into()),
    )
}

fn invalid_entry(path: &Path, line_number: usize, message: impl Into<String>) -> SessionError {
    SessionError::new(
        SessionErrorCode::InvalidEntry,
        format!(
            "session store {}: {EVENTS_FILE} line {line_number} {}",
            path.display(),
            message.into()
        ),
    )
}

impl SessionStorage for SessionDirStorage {
    type Metadata = SessionDirMetadata;

    async fn get_metadata(&self) -> Self::Metadata {
        self.metadata.clone()
    }

    async fn get_leaf_id(&self) -> Result<Option<String>, SessionError> {
        if let Some(leaf_id) = &self.index.leaf_id
            && !self.index.by_id.contains_key(leaf_id)
        {
            return Err(SessionError::new(
                SessionErrorCode::InvalidSession,
                format!("Entry {leaf_id} not found"),
            ));
        }
        Ok(self.index.leaf_id.clone())
    }

    async fn set_leaf_id(&mut self, leaf_id: Option<String>) -> Result<(), SessionError> {
        if let Some(leaf_id) = &leaf_id
            && !self.index.by_id.contains_key(leaf_id)
        {
            return Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Entry {leaf_id} not found"),
            ));
        }
        let entry = create_leaf_entry(self.index.leaf_id.clone(), leaf_id.clone(), &self.index.by_id);
        self.append_event(&entry).await?;
        append_to_index(&mut self.index, entry);
        self.touch_summary().await?;
        Ok(())
    }

    async fn create_entry_id(&self) -> String {
        generate_entry_id(&self.index.by_id)
    }

    async fn append_entry(&mut self, entry: SessionTreeEntry) -> Result<(), SessionError> {
        if let SessionTreeEntry::Message { message, .. } = &entry {
            self.append_chat_mirror(message).await?;
            self.append_prompt_history(message).await?;
        }
        self.append_event(&entry).await?;
        append_to_index(&mut self.index, entry);
        self.touch_summary().await?;
        Ok(())
    }

    async fn get_entry(&self, id: &str) -> Option<SessionTreeEntry> {
        self.index.by_id.get(id).cloned()
    }

    async fn find_entries(&self, entry_type: &str) -> Vec<SessionTreeEntry> {
        find_entries(&self.index.entries, entry_type)
    }

    async fn get_label(&self, id: &str) -> Option<String> {
        self.index.labels_by_id.get(id).cloned()
    }

    async fn get_path_to_root(&self, leaf_id: Option<&str>) -> Result<Vec<SessionTreeEntry>, SessionError> {
        get_path_to_root(&self.index.by_id, leaf_id)
    }

    async fn get_entries(&self) -> Vec<SessionTreeEntry> {
        self.index.entries.clone()
    }
}
