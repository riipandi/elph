//! JSONL session file storage (elph v3 format).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;

use crate::session::id::generate_entry_id;
use crate::session::storage_utils::{append_to_index, build_index, create_leaf_entry, find_entries, get_path_to_root};
use crate::session::types::{
    JsonlSessionMetadata, SessionError, SessionErrorCode, SessionIndex, SessionStorage, SessionTreeEntry,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionHeader {
    #[serde(rename = "type")]
    header_type: String,
    version: u8,
    id: String,
    timestamp: String,
    cwd: String,
    #[serde(rename = "parentSession", skip_serializing_if = "Option::is_none")]
    parent_session: Option<String>,
}

#[derive(Clone)]
pub struct JsonlSessionStorage {
    file_path: PathBuf,
    metadata: JsonlSessionMetadata,
    index: SessionIndex,
}

impl JsonlSessionStorage {
    pub async fn open(file_path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let file_path = file_path.as_ref().to_path_buf();
        let content = fs::read_to_string(&file_path)
            .await
            .map_err(|error| storage_error(&file_path, format!("failed to read session: {error}")))?;
        let loaded = parse_jsonl_content(&content, &file_path)?;
        let metadata = header_to_metadata(&loaded.header, &file_path);
        let index = build_index(loaded.entries, loaded.leaf_id)?;
        Ok(Self {
            file_path,
            metadata,
            index,
        })
    }

    pub async fn create(file_path: impl AsRef<Path>, options: JsonlSessionCreateOptions) -> Result<Self, SessionError> {
        let file_path = file_path.as_ref().to_path_buf();
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| storage_error(&file_path, format!("failed to create parent dir: {error}")))?;
        }
        let header = SessionHeader {
            header_type: "session".to_string(),
            version: 3,
            id: options.session_id,
            timestamp: crate::messages::now_iso_timestamp(),
            cwd: options.cwd,
            parent_session: options.parent_session_path,
        };
        let line = serde_json::to_string(&header)
            .map_err(|error| storage_error(&file_path, format!("failed to encode header: {error}")))?;
        fs::write(&file_path, format!("{line}\n"))
            .await
            .map_err(|error| storage_error(&file_path, format!("failed to create session: {error}")))?;
        let metadata = header_to_metadata(&header, &file_path);
        Ok(Self {
            file_path,
            metadata,
            index: build_index(Vec::new(), None)?,
        })
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    async fn append_line(&self, entry: &SessionTreeEntry) -> Result<(), SessionError> {
        let line = serde_json::to_string(entry)
            .map_err(|error| storage_error(&self.file_path, format!("failed to encode entry: {error}")))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .await
            .map_err(|error| storage_error(&self.file_path, format!("failed to open session: {error}")))?;
        file.write_all(format!("{line}\n").as_bytes())
            .await
            .map_err(|error| storage_error(&self.file_path, format!("failed to append entry: {error}")))?;
        file.flush()
            .await
            .map_err(|error| storage_error(&self.file_path, format!("failed to flush session: {error}")))?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct JsonlSessionCreateOptions {
    pub cwd: String,
    pub session_id: String,
    pub parent_session_path: Option<String>,
}

struct LoadedJsonlSession {
    header: SessionHeader,
    entries: Vec<SessionTreeEntry>,
    leaf_id: Option<String>,
}

fn header_to_metadata(header: &SessionHeader, path: &Path) -> JsonlSessionMetadata {
    JsonlSessionMetadata {
        id: header.id.clone(),
        created_at: header.timestamp.clone(),
        cwd: header.cwd.clone(),
        path: path.to_string_lossy().to_string(),
        parent_session_path: header.parent_session.clone(),
    }
}

fn storage_error(path: &Path, message: impl Into<String>) -> SessionError {
    SessionError::new(
        SessionErrorCode::Storage,
        format!("Invalid JSONL session file {}: {}", path.display(), message.into()),
    )
}

fn invalid_session(path: &Path, message: impl Into<String>) -> SessionError {
    SessionError::new(
        SessionErrorCode::InvalidSession,
        format!("Invalid JSONL session file {}: {}", path.display(), message.into()),
    )
}

fn invalid_entry(path: &Path, line_number: usize, message: impl Into<String>) -> SessionError {
    SessionError::new(
        SessionErrorCode::InvalidEntry,
        format!(
            "Invalid JSONL session file {}: line {line_number} {}",
            path.display(),
            message.into()
        ),
    )
}

fn parse_header_line(line: &str, file_path: &Path) -> Result<SessionHeader, SessionError> {
    let parsed: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| invalid_session(file_path, format!("first line is not a valid session header: {error}")))?;
    let header_type = parsed
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_session(file_path, "first line is not a valid session header"))?;
    if header_type != "session" {
        return Err(invalid_session(file_path, "first line is not a valid session header"));
    }
    let version = parsed
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| invalid_session(file_path, "unsupported session version"))?;
    if version != 3 {
        return Err(invalid_session(file_path, "unsupported session version"));
    }
    let id = parsed
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_session(file_path, "session header is missing id"))?;
    let timestamp = parsed
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_session(file_path, "session header is missing timestamp"))?;
    let cwd = parsed
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_session(file_path, "session header is missing cwd"))?;
    let parent_session = parsed
        .get("parentSession")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    Ok(SessionHeader {
        header_type: "session".to_string(),
        version: 3,
        id: id.to_string(),
        timestamp: timestamp.to_string(),
        cwd: cwd.to_string(),
        parent_session,
    })
}

fn parse_entry_line(line: &str, file_path: &Path, line_number: usize) -> Result<SessionTreeEntry, SessionError> {
    let parsed: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| invalid_entry(file_path, line_number, format!("is not valid JSON: {error}")))?;
    let entry_type = parsed
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_entry(file_path, line_number, "is missing entry type"))?;
    let id = parsed
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_entry(file_path, line_number, "is missing entry id"))?;
    let parent_id = parsed.get("parentId");
    if parent_id.is_some() && !parent_id.is_none_or(|value| value.is_null() || value.is_string()) {
        return Err(invalid_entry(file_path, line_number, "has invalid parentId"));
    }
    let timestamp = parsed
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_entry(file_path, line_number, "is missing timestamp"))?;
    if entry_type == "leaf" {
        let target_id = parsed.get("targetId");
        if target_id.is_some() && !target_id.is_none_or(|value| value.is_null() || value.is_string()) {
            return Err(invalid_entry(file_path, line_number, "has invalid targetId"));
        }
    }
    let _ = (id, timestamp);
    serde_json::from_value(parsed)
        .map_err(|error| invalid_entry(file_path, line_number, format!("is not a valid session entry: {error}")))
}

fn parse_jsonl_content(content: &str, file_path: &Path) -> Result<LoadedJsonlSession, SessionError> {
    let lines: Vec<&str> = content.lines().filter(|line| !line.trim().is_empty()).collect();
    if lines.is_empty() {
        return Err(invalid_session(file_path, "missing session header"));
    }
    let header = parse_header_line(lines[0], file_path)?;
    let mut entries = Vec::new();
    let mut leaf_id = None;
    for (index, line) in lines.iter().skip(1).enumerate() {
        let entry = parse_entry_line(line, file_path, index + 2)?;
        leaf_id = crate::session::storage_utils::leaf_id_after_entry(&entry);
        entries.push(entry);
    }
    Ok(LoadedJsonlSession {
        header,
        entries,
        leaf_id,
    })
}

pub async fn load_jsonl_session_metadata(file_path: impl AsRef<Path>) -> Result<JsonlSessionMetadata, SessionError> {
    let file_path = file_path.as_ref();
    let content = fs::read_to_string(file_path)
        .await
        .map_err(|error| storage_error(file_path, format!("failed to read session header: {error}")))?;
    let mut lines = content.lines().filter(|line| !line.trim().is_empty());
    let first = lines
        .next()
        .ok_or_else(|| invalid_session(file_path, "missing session header"))?;
    let header = parse_header_line(first, file_path)?;
    Ok(header_to_metadata(&header, file_path))
}

impl SessionStorage for JsonlSessionStorage {
    type Metadata = JsonlSessionMetadata;

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
        self.append_line(&entry).await?;
        append_to_index(&mut self.index, entry);
        Ok(())
    }

    async fn create_entry_id(&self) -> String {
        generate_entry_id(&self.index.by_id)
    }

    async fn append_entry(&mut self, entry: SessionTreeEntry) -> Result<(), SessionError> {
        self.append_line(&entry).await?;
        append_to_index(&mut self.index, entry);
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
