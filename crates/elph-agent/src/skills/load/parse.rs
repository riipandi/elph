//! Skill frontmatter parsing and validation.

use serde::Deserialize;

use crate::harness::types::{Result, err, ok};

const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const MAX_COMPATIBILITY_LENGTH: usize = 500;

pub(super) fn validate_name(name: &str, parent_dir_name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if name != parent_dir_name {
        errors.push(format!(
            "name \"{name}\" does not match parent directory \"{parent_dir_name}\""
        ));
    }
    if name.len() > MAX_NAME_LENGTH {
        errors.push(format!("name exceeds {MAX_NAME_LENGTH} characters ({})", name.len()));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        errors.push("name contains invalid characters (must be lowercase a-z, 0-9, hyphens only)".to_string());
    }
    if name.starts_with('-') || name.ends_with('-') {
        errors.push("name must not start or end with a hyphen".to_string());
    }
    if name.contains("--") {
        errors.push("name must not contain consecutive hyphens".to_string());
    }
    errors
}

pub(super) fn validate_description(description: Option<&str>) -> Vec<String> {
    let mut errors = Vec::new();
    match description {
        None => errors.push("description is required".to_string()),
        Some(value) if value.trim().is_empty() => errors.push("description is required".to_string()),
        Some(value) if value.len() > MAX_DESCRIPTION_LENGTH => errors.push(format!(
            "description exceeds {MAX_DESCRIPTION_LENGTH} characters ({})",
            value.len()
        )),
        _ => {}
    }
    errors
}

pub(super) fn validate_compatibility(compatibility: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if compatibility.len() > MAX_COMPATIBILITY_LENGTH {
        errors.push(format!(
            "compatibility exceeds {MAX_COMPATIBILITY_LENGTH} characters ({})",
            compatibility.len()
        ));
    }
    errors
}

pub(super) struct ParsedFrontmatter<T> {
    pub frontmatter: T,
    pub body: String,
}

pub(super) fn parse_frontmatter<T: for<'de> Deserialize<'de> + Default>(
    content: &str,
) -> Result<ParsedFrontmatter<T>, String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.starts_with("---") {
        return ok(ParsedFrontmatter {
            frontmatter: T::default(),
            body: normalized,
        });
    }
    let Some(end_index) = normalized[3..].find("\n---").map(|index| index + 3) else {
        return ok(ParsedFrontmatter {
            frontmatter: T::default(),
            body: normalized,
        });
    };
    let yaml_string = &normalized[4..end_index];
    let body = normalized[end_index + 4..].trim().to_string();
    let frontmatter: T = match serde_yaml::from_str(yaml_string) {
        Ok(value) => value,
        Err(error) => return err(to_error(error)),
    };
    ok(ParsedFrontmatter { frontmatter, body })
}

fn to_error(error: serde_yaml::Error) -> String {
    error.to_string()
}
