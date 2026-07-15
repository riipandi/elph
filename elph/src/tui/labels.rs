//! Footer and header label formatting.

use crate::platform::Paths;

pub fn session_label(resume_id: Option<&str>) -> String {
    let id = resume_id.unwrap_or("019f631516e6g29o");
    format!("Session: {id} | turn: 0")
}

pub fn project_footer_label(paths: &Paths) -> String {
    let name = paths.project_dir().file_name().and_then(|s| s.to_str()).unwrap_or("?");
    format!("~ {name} [branch-name]")
}

pub fn model_footer_label(provider_id: Option<&str>, model_id: Option<&str>) -> String {
    match (provider_id, model_id) {
        (Some(provider), Some(model)) => format!("{provider}/{model}"),
        (None, Some(model)) => model.to_string(),
        _ => "no model selected".to_string(),
    }
}
