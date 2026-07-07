use elph_agent::harness::format_skills_for_system_prompt;
use elph_agent::harness::types::Skill;

fn visible_skill() -> Skill {
    Skill {
        name: "visible".to_string(),
        description: "Use <this> & that".to_string(),
        content: "visible content".to_string(),
        file_path: "/skills/visible/SKILL.md".to_string(),
        disable_model_invocation: false,
    }
}

fn second_skill() -> Skill {
    Skill {
        name: "second".to_string(),
        description: "Second skill".to_string(),
        content: "second content".to_string(),
        file_path: "/skills/second/SKILL.md".to_string(),
        disable_model_invocation: false,
    }
}

fn disabled_skill() -> Skill {
    Skill {
        name: "hidden".to_string(),
        description: "Hidden".to_string(),
        content: "hidden content".to_string(),
        file_path: "/skills/hidden/SKILL.md".to_string(),
        disable_model_invocation: true,
    }
}

#[test]
fn format_skills_for_system_prompt_orders_visible_skills() {
    let formatted = format_skills_for_system_prompt(&[visible_skill(), disabled_skill(), second_skill()]);

    let expected = "\
The following skills provide specialized instructions for specific tasks.
Read the full skill file when the task matches its description.
When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.

<available_skills>
  <skill>
    <name>visible</name>
    <description>Use &lt;this&gt; &amp; that</description>
    <location>/skills/visible/SKILL.md</location>
  </skill>
  <skill>
    <name>second</name>
    <description>Second skill</description>
    <location>/skills/second/SKILL.md</location>
  </skill>
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
    }]);

    assert!(formatted.contains(
        "<name>a&amp;b</name>\n    <description>Quote &quot;double&quot; and &apos;single&apos;</description>\n    <location>/skills/&lt;bad&gt;&amp;&quot;quote&quot;/SKILL.md</location>"
    ));
}
