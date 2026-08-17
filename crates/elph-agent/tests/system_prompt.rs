use elph_agent::agent::harness::format_skills_for_system_prompt;
use elph_agent::agent::harness::types::Skill;

fn visible_skill() -> Skill {
    Skill {
        name: "visible".to_string(),
        description: "Use <this> & that".to_string(),
        content: "visible content".to_string(),
        file_path: "/skills/visible/SKILL.md".to_string(),
        disable_model_invocation: false,
        license: None,
        compatibility: None,
        metadata: None,
        allowed_tools: None,
        argument_hint: None,
    }
}

fn second_skill() -> Skill {
    Skill {
        name: "second".to_string(),
        description: "Second skill".to_string(),
        content: "second content".to_string(),
        file_path: "/skills/second/SKILL.md".to_string(),
        disable_model_invocation: false,
        license: None,
        compatibility: None,
        metadata: None,
        allowed_tools: None,
        argument_hint: None,
    }
}

fn disabled_skill() -> Skill {
    Skill {
        name: "hidden".to_string(),
        description: "Hidden".to_string(),
        content: "hidden content".to_string(),
        file_path: "/skills/hidden/SKILL.md".to_string(),
        disable_model_invocation: true,
        license: None,
        compatibility: None,
        metadata: None,
        allowed_tools: None,
        argument_hint: None,
    }
}

#[test]
fn format_skills_for_system_prompt_orders_visible_skills() {
    let formatted = format_skills_for_system_prompt(&[visible_skill(), disabled_skill(), second_skill()]);

    let expected = "\
<available_skills note=\"On match: read SKILL.md fully before acting, resolve relative refs from its dir. Skip loosely related. No match -> proceed without one, don't browse to be thorough.\">
  <skill name=\"visible\" path=\"/skills/visible/SKILL.md\"/>
  <skill name=\"second\" path=\"/skills/second/SKILL.md\"/>
</available_skills>";
    assert_eq!(formatted, expected);
}

#[test]
fn format_skills_for_system_prompt_returns_empty_when_all_disabled() {
    assert_eq!(format_skills_for_system_prompt(&[disabled_skill()]), "");
}

#[test]
fn format_skills_for_system_prompt_escapes_xml_fields() {
    let formatted = format_skills_for_system_prompt(&[Skill {
        name: "a&b".to_string(),
        description: "Quote \"double\" and 'single'".to_string(),
        content: "content".to_string(),
        file_path: "/skills/<bad>&\"quote\"/SKILL.md".to_string(),
        disable_model_invocation: false,
        license: None,
        compatibility: None,
        metadata: None,
        allowed_tools: None,
        argument_hint: None,
    }]);

    // XML escaping is applied to name and path attributes only (description is no longer included).
    assert!(
        formatted.contains("<skill name=\"a&amp;b\" path=\"/skills/&lt;bad&gt;&amp;&quot;quote&quot;/SKILL.md\"/>")
    );
}
