//! Resource formatting tests.

use elph_agent::{PromptTemplate, Skill, format_prompt_template_invocation, format_skill_invocation};

#[test]
fn format_skill_invocation_includes_additional_instructions() {
    let skill = Skill {
        name: "inspect".to_string(),
        description: "Inspect things".to_string(),
        content: "Use inspection tools.".to_string(),
        file_path: "/project/.elph/skills/inspect/SKILL.md".to_string(),
        disable_model_invocation: false,
        license: Some("MIT".to_string()),
        compatibility: None,
        metadata: None,
        allowed_tools: Some(vec!["read".to_string(), "grep".to_string()]),
    };

    assert_eq!(
        format_skill_invocation(&skill, Some("Check errors.")),
        "<skill name=\"inspect\" location=\"/project/.elph/skills/inspect/SKILL.md\">\n\
         <license>MIT</license>\n\
         <allowed-tools>read grep</allowed-tools>\n\
         References are relative to /project/.elph/skills/inspect.\n\n\
         Use inspection tools.\n\
         </skill>\n\n\
         Check errors."
    );
}

#[test]
fn format_prompt_template_invocation_substitutes_positional_arguments() {
    let template = PromptTemplate {
        name: "review".to_string(),
        description: String::new(),
        content: "Review $1 with $ARGUMENTS".to_string(),
    };

    assert_eq!(
        format_prompt_template_invocation(&template, &["a.ts".to_string(), "care".to_string()]),
        "Review a.ts with a.ts care"
    );
}
