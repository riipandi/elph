package prompt

import (
	"strings"

	"github.com/riipandi/elph/pkg/skill"
)

// Skill describes a discoverable SKILL.md entry for the system prompt.
type Skill struct {
	Name        string
	Description string
	Location    string
}

// DiscoverSkills loads skills from user and project scopes per agentskills.io
// (including ~/.agents/skills and <workDir>/.agents/skills). Project skills
// override user skills with the same name.
func DiscoverSkills(workDir string) []Skill {
	defs := skill.Discover(workDir)
	out := make([]Skill, len(defs))
	for i, def := range defs {
		out[i] = Skill{
			Name:        def.Name,
			Description: def.Description,
			Location:    def.Location,
		}
	}
	return out
}

func formatSkillsSection(skills []Skill) string {
	if len(skills) == 0 {
		return ""
	}

	var b strings.Builder
	b.WriteString("## Skills\n")
	b.WriteString("The following skills provide specialized instructions for specific tasks.\n")
	b.WriteString("When a task matches a skill's description, call the Skill tool with the skill's name to load its full instructions.\n")
	b.WriteString("When a skill references relative paths, resolve them against the skill directory (parent of SKILL.md) and use absolute paths in tool calls.\n\n")
	b.WriteString("Skill instructions are internal workflow guidance — not a script for user-visible text.\n")
	b.WriteString("In user-facing replies, follow the system prompt Output section. It overrides skill templates and examples:\n")
	b.WriteString("- Answer directly in your normal voice — never prefix with skill names, tags, or mode labels (e.g. ASIDE:, [code-review], Skill: aside).\n")
	b.WriteString("- Never announce loading, activating, invoking, or completing a skill.\n")
	b.WriteString("- Never add meta footers such as \"back to task\", \"returning to main task\", or similar unless the user explicitly requested that exact phrasing.\n")
	b.WriteString("- Use skill text for what to do and how to reason; ignore decorative output framing shown in skill examples.\n\n")
	b.WriteString("<available_skills>\n")
	for _, skill := range skills {
		b.WriteString("  <skill>\n")
		b.WriteString("    <name>")
		b.WriteString(escapeXML(skill.Name))
		b.WriteString("</name>\n")
		b.WriteString("    <description>")
		b.WriteString(escapeXML(collapseWhitespace(skill.Description)))
		b.WriteString("</description>\n")
		b.WriteString("    <location>")
		b.WriteString(escapeXML(skill.Location))
		b.WriteString("</location>\n")
		b.WriteString("  </skill>\n")
	}
	b.WriteString("</available_skills>")
	return b.String()
}

func collapseWhitespace(s string) string {
	return strings.Join(strings.Fields(s), " ")
}

func escapeXML(s string) string {
	s = strings.ReplaceAll(s, "&", "&amp;")
	s = strings.ReplaceAll(s, "<", "&lt;")
	s = strings.ReplaceAll(s, ">", "&gt;")
	return s
}
