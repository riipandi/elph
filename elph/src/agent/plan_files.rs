//! Plan file persistence: save approved plans to `.elph/plans/` with frontmatter.

use std::fs;
use std::io::Write;

use anyhow::{Context, Result};

use crate::platform::Paths;

/// Save plan text to `.elph/plans/plan-YYYYMMDD_HHmm.md` with YAML frontmatter.
///
/// Returns the absolute path to the saved file.
pub fn save_plan_to_disk(plan_text: &str, paths: &Paths) -> Result<String> {
    let plans_dir = paths.plans_dir();
    fs::create_dir_all(&plans_dir).with_context(|| format!("Failed to create plans dir: {}", plans_dir.display()))?;

    let subject = extract_plan_subject(plan_text);
    let now = chrono::Local::now();
    let timestamp = now.format("%Y%m%d_%H%M").to_string();
    let filename = format!("plan-{timestamp}.md");
    let file_path = plans_dir.join(&filename);

    let frontmatter = format!(
        "---\nSubject: {subject}\nStatus: planned\nCreated: {}\nUpdated: {}\n---\n\n",
        now.format("%Y-%m-%d %H:%M"),
        now.format("%Y-%m-%d %H:%M"),
    );

    let mut file =
        fs::File::create(&file_path).with_context(|| format!("Failed to create plan file: {}", file_path.display()))?;
    file.write_all(frontmatter.as_bytes())
        .context("Failed to write plan frontmatter")?;
    file.write_all(plan_text.as_bytes())
        .context("Failed to write plan body")?;

    let canonical = file_path.canonicalize().context("Failed to canonicalize plan path")?;
    Ok(canonical.to_string_lossy().to_string())
}

/// Extract a subject/title from the first heading in the plan text.
///
/// Looks for the first line starting with `# `, then `## `, then falls back
/// to `"Plan"`.
pub fn extract_plan_subject(plan_text: &str) -> String {
    for line in plan_text.lines() {
        let trimmed = line.trim();
        if let Some(text) = trimmed.strip_prefix("# ") {
            return text.trim().to_string();
        }
        if let Some(text) = trimmed.strip_prefix("## ") {
            return text.trim().to_string();
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
    fn falls_back_to_plan() {
        assert_eq!(extract_plan_subject("Just some text"), "Plan");
        assert_eq!(extract_plan_subject(""), "Plan");
    }

    #[test]
    fn save_plan_to_disk_creates_file() {
        use std::path::Path;
        let tmp = tempfile::tempdir().expect("tempdir");
        let project = tmp.path().join("repo");
        let paths = Paths::from_dirs(tmp.path().join("config"), tmp.path().join("data"), project.clone());

        let plan_text = "# Test Plan\nDo something";
        let saved = save_plan_to_disk(plan_text, &paths).expect("save");

        let saved_path = Path::new(&saved);
        assert!(saved_path.exists(), "plan file exists");

        let contents = fs::read_to_string(saved_path).expect("read");
        assert!(contents.contains("Subject: Test Plan"));
        assert!(contents.contains("Status: planned"));
        assert!(contents.contains("Created:"));
        assert!(contents.contains("Updated:"));
        assert!(contents.contains(plan_text));
    }
}
