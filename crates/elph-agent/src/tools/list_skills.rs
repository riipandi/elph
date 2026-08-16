//! List available skills — on-demand skill catalog for the model.
//!
//! Unlike `list_available_tools` (which is a native tool snapshot), the skill
//! set lives in `AgentHarnessResources` and can change across workspace reloads.
//! This tool reads the current skill list the same way the system-prompt
//! `<available_skills>` block does, and offers an optional relevance filter
//! (`scope: project` → only show skills whose project root matches `cwd`).

use elph_ai::Tool;

use serde_json::json;

use crate::agent::harness::types::Skill;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build the `<available_skills>`-style XML block for `list_skills` output.
///
/// Follows the [Agent Skills specification](https://agentskills.io/specification):
/// `name` + `path` are attributes; all frontmatter fields are child elements.
pub fn format_skill_catalog(skills: &[Skill]) -> String {
    let mut out = String::from("<available_skills>\n");
    for skill in skills {
        out.push_str(&format!(
            "  <skill name=\"{}\" path=\"{}\">\n",
            escape_xml(&skill.name),
            escape_xml(&skill.file_path),
        ));
        out.push_str(&format!("    <description>{}</description>\n", escape_xml(&skill.description)));
        if let Some(ref license) = skill.license {
            out.push_str(&format!("    <license>{}</license>\n", escape_xml(license)));
        }
        if let Some(ref compatibility) = skill.compatibility {
            out.push_str(&format!("    <compatibility>{}</compatibility>\n", escape_xml(compatibility)));
        }
        if let Some(ref allowed_tools) = skill.allowed_tools
            && !allowed_tools.is_empty()
        {
            out.push_str(&format!(
                "    <allowed-tools>{}</allowed-tools>\n",
                escape_xml(&allowed_tools.join(" "))
            ));
        }
        if let Some(ref metadata) = skill.metadata {
            for (key, value) in metadata {
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.push_str(&format!(
                    "    <metadata key=\"{}\" value=\"{}\"/>\n",
                    escape_xml(key),
                    escape_xml(&value_str)
                ));
            }
        }
        out.push_str("  </skill>\n");
    }
    out.push_str("</available_skills>");
    out
}

/// Create the `list_skills` tool from the current skill set snapshot.
///
/// The optional `relevance` argument accepts `"project"` or `"global"` to filter
/// by `metadata.scope`; omitting it lists every skill regardless of scope.
pub fn create_list_skills_tool(skills: Vec<Skill>) -> AgentTool {
    let snapshot = skills;

    simple_tool(
        Tool {
            name: "list_skills".into(),
            constrained_sampling: None,
            description:
                "Lists all skills the agent can invoke, including their names, descriptions, and file locations. \
                 Use this when you need the full skill catalog or to rediscover a skill that is not advertised in the \
                 current <available_skills> block."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "relevance": {
                        "type": "string",
                        "enum": ["all", "project", "global"],
                        "description": "Optional: `project` = only skills tagged metadata.scope: project for this working tree; `global` = only always-on skills; `all` (default) = everything."
                    }
                },
                "additionalProperties": false
            }),
        },
        "list_skills",
        move |_, args| {
            let snapshot = snapshot.clone();
            Box::pin(async move {
                let relevance = args
                    .get("relevance")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("all");

                let filtered: Vec<Skill> = snapshot
                    .iter()
                    .filter(|skill| match relevance {
                        "project" => {
                            skill
                                .metadata
                                .as_ref()
                                .and_then(|m| m.get(crate::agent::harness::system_prompt::SKILL_SCOPE_METADATA_KEY))
                                .and_then(serde_json::Value::as_str)
                                == Some("project")
                        }
                        "global" => {
                            !skill.disable_model_invocation
                                && skill
                                    .metadata
                                    .as_ref()
                                    .and_then(|m| m.get(crate::agent::harness::system_prompt::SKILL_SCOPE_METADATA_KEY))
                                    .and_then(serde_json::Value::as_str)
                                    != Some("project")
                        }
                        _ => !skill.disable_model_invocation,
                    })
                    .cloned()
                    .collect();

                Ok(AgentToolResult::text(format_skill_catalog(&filtered)))
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str, scope: Option<&str>, disabled: bool) -> Skill {
        let metadata = scope.map(|value| {
            let mut map = std::collections::HashMap::new();
            map.insert(
                crate::agent::harness::system_prompt::SKILL_SCOPE_METADATA_KEY.to_string(),
                json!(value),
            );
            map
        });
        Skill {
            name: name.to_string(),
            description: format!("{name} description"),
            content: "# body".into(),
            file_path: format!("/r/.agents/skills/{name}/SKILL.md"),
            disable_model_invocation: disabled,
            license: None,
            compatibility: None,
            metadata,
            allowed_tools: None,
            argument_hint: None,
        }
    }

    fn run(skills: Vec<Skill>, args: serde_json::Value) -> String {
        let tool = create_list_skills_tool(skills);
        let env = std::sync::Arc::new(crate::runtime::local_env::LocalExecutionEnv::new("/tmp"));
        let ctx = crate::types::ToolContext::new(env);
        let fut = (tool.execute)(String::new(), args, None, None, ctx);
        let result = futures::executor::block_on(fut).expect("runs");
        match result.content.first().expect("content") {
            crate::types::ToolResultContent::Text(t) => t.text.clone(),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn lists_all_skills_by_default() {
        let text = run(
            vec![
                skill("a", Some("project"), false),
                skill("b", None, false),
                skill("c", Some("global"), false),
            ],
            json!({}),
        );
        assert!(text.contains("<skill name=\"a\""));
        assert!(text.contains("<skill name=\"b\""));
        assert!(text.contains("<skill name=\"c\""));
    }

    #[test]
    fn relevance_project_filters_to_scope() {
        let text = run(
            vec![
                skill("a", Some("project"), false),
                skill("b", None, false),
                skill("c", Some("global"), false),
            ],
            json!({ "relevance": "project" }),
        );
        assert!(text.contains("<skill name=\"a\""));
        assert!(!text.contains("<skill name=\"b\""));
        assert!(!text.contains("<skill name=\"c\""));
    }

    #[test]
    fn relevance_global_excludes_project_scoped() {
        let text = run(
            vec![
                skill("a", Some("project"), false),
                skill("b", None, false),
                skill("c", Some("global"), false),
            ],
            json!({ "relevance": "global" }),
        );
        assert!(!text.contains("<skill name=\"a\""));
        assert!(text.contains("<skill name=\"b\""));
        assert!(text.contains("<skill name=\"c\""));
    }

    #[test]
    fn disabled_skills_are_hidden() {
        let text = run(vec![skill("hidden", None, true), skill("visible", None, false)], json!({}));
        assert!(!text.contains("<skill name=\"hidden\""));
        assert!(text.contains("<skill name=\"visible\""));
    }

    #[test]
    fn output_has_structured_xml_with_all_spec_fields() {
        let mut meta = std::collections::HashMap::new();
        meta.insert("version".to_string(), json!("1.0"));
        let skill = Skill {
            name: "pdf-processing".to_string(),
            description: "Extract PDF text and tables.".to_string(),
            content: "# PDF".into(),
            file_path: "/skills/pdf/SKILL.md".to_string(),
            disable_model_invocation: false,
            license: Some("Apache-2.0".to_string()),
            compatibility: Some("Requires poppler".to_string()),
            metadata: Some(meta),
            allowed_tools: Some(vec!["read".to_string(), "shell_exec".to_string()]),
            argument_hint: None,
        };
        let text = run(vec![skill], json!({}));
        assert!(text.contains("<available_skills>"));
        assert!(text.contains("<skill name=\"pdf-processing\" path=\"/skills/pdf/SKILL.md\">"));
        assert!(text.contains("<description>Extract PDF text and tables.</description>"));
        assert!(text.contains("<license>Apache-2.0</license>"));
        assert!(text.contains("<compatibility>Requires poppler</compatibility>"));
        assert!(text.contains("<allowed-tools>read shell_exec</allowed-tools>"));
        assert!(text.contains("<metadata key=\"version\" value=\"1.0\"/>"));
        assert!(text.contains("</skill>"));
        assert!(text.contains("</available_skills>"));
    }
}
