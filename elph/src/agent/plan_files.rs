//! Plan file persistence: save approved plans to `.elph/plans/` with frontmatter.

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::platform::Paths;

/// Save plan text to `.elph/plans/plan-YYYYMMDD_HHmm.md` with YAML frontmatter.
///
/// `session_id` is optional — when provided, it is stored in the `Session` field.
/// For plans created before a session is ready (e.g. `ImplementFresh`), pass `None`
/// and update the field later via [`update_plan_frontmatter`].
///
/// Returns the absolute path to the saved file.
pub fn save_plan_to_disk(plan_text: &str, paths: &Paths, session_id: Option<&str>) -> Result<String> {
    let plans_dir = paths.plans_dir();
    fs::create_dir_all(&plans_dir).with_context(|| format!("Failed to create plans dir: {}", plans_dir.display()))?;

    let subject = extract_plan_subject(plan_text);
    let now = chrono::Local::now();
    let timestamp = now.format("%Y%m%d_%H%M").to_string();
    let filename = format!("plan-{timestamp}.md");
    let file_path = plans_dir.join(&filename);

    let session_line = session_id
        .filter(|s| !s.trim().is_empty())
        .map(|sid| format!("Session: {sid}\n"))
        .unwrap_or_default();
    let frontmatter = format!(
        "---\nSubject: {subject}\n{session_line}Status: planned\nCreated: {}\nUpdated: {}\n---\n\n",
        now.format("%Y-%m-%d %H:%M"),
        now.format("%Y-%m-%d %H:%M"),
    );

    let mut file =
        fs::File::create(&file_path).with_context(|| format!("Failed to create plan file: {}", file_path.display()))?;
    file.write_all(frontmatter.as_bytes())
        .context("Failed to write plan frontmatter")?;
    file.write_all(plan_text.as_bytes())
        .context("Failed to write plan body")?;
    file.flush().context("Failed to flush plan file")?;
    file.sync_all().context("Failed to sync plan file to disk")?;

    let canonical = file_path.canonicalize().context("Failed to canonicalize plan path")?;
    Ok(canonical.to_string_lossy().to_string())
}

/// Update `Status`, `Updated`, and optionally `Session` fields in a saved plan file's YAML frontmatter.
///
/// Parses the frontmatter (delimited by `---` lines), replaces the target fields,
/// and writes the file back preserving the body content.
///
/// Returns an error if the file doesn't exist or has no valid frontmatter.
pub fn update_plan_frontmatter(
    file_path: &str,
    new_status: &str,
    new_updated: &str,
    session_id: Option<&str>,
) -> Result<()> {
    let path = Path::new(file_path);
    if !path.exists() {
        anyhow::bail!("Plan file not found: {file_path}");
    }
    let content = fs::read_to_string(path).with_context(|| format!("Failed to read plan file: {file_path}"))?;

    let mut lines: Vec<String> = content.lines().map(ToOwned::to_owned).collect();
    let mut in_frontmatter = false;
    let mut frontmatter_end: Option<usize> = None;
    let mut status_found = false;
    let mut updated_found = false;
    let mut session_id_found = false;

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == "---" {
            if !in_frontmatter {
                in_frontmatter = true;
                i += 1;
                continue;
            } else {
                frontmatter_end = Some(i);
                break;
            }
        }
        if in_frontmatter {
            if lines[i].starts_with("Status:") {
                lines[i] = format!("Status: {new_status}");
                status_found = true;
            }
            if lines[i].starts_with("Updated:") {
                lines[i] = format!("Updated: {new_updated}");
                updated_found = true;
            }
            if let Some(sid) = session_id.filter(|s| !s.trim().is_empty())
                && lines[i].starts_with("Session:")
            {
                lines[i] = format!("Session: {sid}");
                session_id_found = true;
            }
        }
        i += 1;
    }

    let end = frontmatter_end.ok_or_else(|| anyhow::anyhow!("Plan file has no closing frontmatter delimiter"))?;

    // If fields were missing, append them before the closing `---`.
    let mut insertions: Vec<String> = Vec::new();
    if !status_found {
        insertions.push(format!("Status: {new_status}"));
    }
    if !updated_found {
        insertions.push(format!("Updated: {new_updated}"));
    }
    if let Some(sid) = session_id.filter(|s| !s.trim().is_empty())
        && !session_id_found
    {
        insertions.push(format!("Session: {sid}"));
    }
    if !insertions.is_empty() {
        for (j, line) in insertions.into_iter().enumerate() {
            lines.insert(end + j, line);
        }
    }

    let new_content = lines.join("\n") + "\n";
    fs::write(path, new_content).with_context(|| format!("Failed to write plan file: {file_path}"))?;
    Ok(())
}

/// Extract a descriptive subject/title from plan text.
///
/// Priority:
/// 1. First `# ` heading
/// 2. First `## ` heading
/// 3. First `### ` heading
/// 4. First non-empty, non-heading line (truncated to 80 chars)
/// 5. `"Plan"` (fallback)
pub fn extract_plan_subject(plan_text: &str) -> String {
    let mut first_body_line: Option<&str> = None;

    for line in plan_text.lines() {
        let trimmed = line.trim();

        // Skip blank lines.
        if trimmed.is_empty() {
            continue;
        }

        // H1 heading — best possible subject.
        if let Some(text) = trimmed.strip_prefix("# ") {
            let subject = text.trim().to_string();
            if !subject.is_empty() {
                return subject;
            }
        }

        // H2 heading — good subject.
        if let Some(text) = trimmed.strip_prefix("## ") {
            let subject = text.trim().to_string();
            if !subject.is_empty() {
                return subject;
            }
        }

        // H3 heading — acceptable subject.
        if let Some(text) = trimmed.strip_prefix("### ") {
            let subject = text.trim().to_string();
            if !subject.is_empty() {
                return subject;
            }
        }

        // First non-heading line: save for fallback.
        if first_body_line.is_none()
            && !trimmed.starts_with('#')
            && !trimmed.starts_with('-')
            && !trimmed.starts_with('*')
        {
            first_body_line = Some(trimmed);
        }
    }

    // Fallback: first meaningful line, truncated.
    if let Some(line) = first_body_line {
        let cleaned = line.trim_matches(&['-', '*', ' ', '\t', '`', '"', '\''][..]);
        if !cleaned.is_empty() {
            let truncated: String = cleaned.chars().take(80).collect();
            return if cleaned.len() > 80 {
                format!("{truncated}…")
            } else {
                truncated
            };
        }
    }

    "Plan".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_subject_from_h1() {
        let text = "# My Great Plan\nStep 1: ...";
        assert_eq!(extract_plan_subject(text), "My Great Plan");
    }

    #[test]
    fn extracts_subject_from_h2_fallback() {
        let text = "## Step-by-step\nDo thing A\nDo thing B";
        assert_eq!(extract_plan_subject(text), "Step-by-step");
    }

    #[test]
    fn extracts_subject_from_h3() {
        let text = "### Refactor database layer\nStep 1: migrate schema";
        assert_eq!(extract_plan_subject(text), "Refactor database layer");
    }

    #[test]
    fn falls_back_to_body_line() {
        let text = "This plan covers the implementation of user authentication …";
        assert_eq!(
            extract_plan_subject(text),
            "This plan covers the implementation of user authentication …"
        );
    }

    #[test]
    fn falls_back_to_plan() {
        assert_eq!(extract_plan_subject("\n\n\n\n"), "Plan");
        assert_eq!(extract_plan_subject(""), "Plan");
    }

    #[test]
    fn truncates_long_lines() {
        let long = "a".repeat(200);
        let result = extract_plan_subject(&long);
        // 80 chars + ellipsis (1 char) = 81 chars
        assert_eq!(result.chars().count(), 81);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn skips_list_items_for_fallback() {
        let text = "- item one\n- item two\nDo the real work";
        let result = extract_plan_subject(text);
        assert_eq!(result, "Do the real work");
    }

    #[test]
    fn prefers_h1_over_body() {
        let text = "# Overview\nThis plan describes the changes needed.";
        assert_eq!(extract_plan_subject(text), "Overview");
    }

    #[test]
    fn prefers_h2_over_body() {
        let text = "Some intro text\n\n## Implementation\nDo step 1\nDo step 2";
        assert_eq!(extract_plan_subject(text), "Implementation");
    }

    #[test]
    fn save_plan_to_disk_creates_file() {
        use std::path::Path;
        let tmp = tempfile::tempdir().expect("tempdir");
        let project = tmp.path().join("repo");
        let paths = Paths::from_dirs(tmp.path().join("config"), tmp.path().join("data"), project.clone());

        let plan_text = "# Test Plan\nDo something";
        let saved = save_plan_to_disk(plan_text, &paths, Some("sess-abc123")).expect("save");

        let saved_path = Path::new(&saved);
        assert!(saved_path.exists(), "plan file exists");

        let contents = fs::read_to_string(saved_path).expect("read");
        assert!(contents.contains("Subject: Test Plan"));
        assert!(contents.contains("Session: sess-abc123"));
        assert!(contents.contains("Status: planned"));
        assert!(contents.contains("Created:"));
        assert!(contents.contains("Updated:"));
        assert!(contents.contains(plan_text));
    }

    #[test]
    fn save_plan_to_disk_without_session_id() {
        use std::path::Path;
        let tmp = tempfile::tempdir().expect("tempdir");
        let project = tmp.path().join("repo");
        let paths = Paths::from_dirs(tmp.path().join("config"), tmp.path().join("data"), project.clone());

        let plan_text = "# No Session Plan\nJust testing";
        let saved = save_plan_to_disk(plan_text, &paths, None).expect("save");

        let saved_path = Path::new(&saved);
        let contents = fs::read_to_string(saved_path).expect("read");
        assert!(contents.contains("Subject: No Session Plan"));
        assert!(contents.contains("Status: planned"));
        assert!(!contents.contains("Session:")); // No session id line
    }

    #[test]
    fn update_plan_frontmatter_replaces_status_and_updated() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("plan-test.md");
        let path_str = path.to_string_lossy().to_string();

        let content = concat!(
            "---\n",
            "Subject: Test\n",
            "Status: planned\n",
            "Created: 2026-07-28 23:00\n",
            "Updated: 2026-07-28 23:00\n",
            "---\n\n",
            "## Step 1\n",
            "Do the thing\n",
        );
        fs::write(&path, content).expect("write");

        update_plan_frontmatter(&path_str, "in_progress", "2026-07-28 23:30", Some("sess-xyz")).expect("update");

        let updated = fs::read_to_string(&path).expect("read");
        assert!(updated.contains("Status: in_progress"));
        assert!(updated.contains("Updated: 2026-07-28 23:30"));
        assert!(updated.contains("Session: sess-xyz"));
        assert!(updated.contains("Subject: Test")); // unchanged
        assert!(updated.contains("Created: 2026-07-28 23:00")); // unchanged
        assert!(updated.contains("## Step 1")); // body preserved
        assert!(updated.contains("Do the thing")); // body preserved
    }

    #[test]
    fn update_plan_frontmatter_replaces_existing_session_id() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("plan-session.md");
        let path_str = path.to_string_lossy().to_string();

        let content = concat!(
            "---\n",
            "Subject: Test\n",
            "Session: old-session\n",
            "Status: planned\n",
            "Created: 2026-07-28 23:00\n",
            "Updated: 2026-07-28 23:00\n",
            "---\n\n",
            "Body\n",
        );
        fs::write(&path, content).expect("write");

        update_plan_frontmatter(&path_str, "completed", "2026-07-29 00:00", Some("new-session-id")).expect("update");

        let updated = fs::read_to_string(&path).expect("read");
        assert!(updated.contains("Session: new-session-id"));
        assert!(!updated.contains("Session: old-session"));
    }

    #[test]
    fn update_plan_frontmatter_appends_missing_fields() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("plan-minimal.md");
        let path_str = path.to_string_lossy().to_string();

        // Minimal frontmatter without Status/Updated
        let content = "---\nSubject: Test\n---\n\nBody text\n";
        fs::write(&path, content).expect("write");

        update_plan_frontmatter(&path_str, "completed", "2026-07-29 00:00", None).expect("update");

        let updated = fs::read_to_string(&path).expect("read");
        assert!(updated.contains("Status: completed"));
        assert!(updated.contains("Updated: 2026-07-29 00:00"));
        assert!(updated.contains("Body text"));
    }

    #[test]
    fn update_plan_frontmatter_errors_on_missing_file() {
        let result = update_plan_frontmatter("/nonexistent/plan.md", "done", "now", None);
        assert!(result.is_err());
    }

    #[test]
    fn update_plan_frontmatter_errors_on_no_frontmatter() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("no-frontmatter.md");
        let path_str = path.to_string_lossy().to_string();
        fs::write(&path, "Just body text\n").expect("write");

        let result = update_plan_frontmatter(&path_str, "done", "now", None);
        assert!(result.is_err());
    }
}
