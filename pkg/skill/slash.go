package skill

import "strings"

// SlashAgentPrompt returns the user-turn content sent to the model for /skill:<name>.
func SlashAgentPrompt(def Definition, args string) string {
	return FormatActivation(def, args)
}

// SlashDetailBody returns collapsible detail text for /skill:<name> in the TUI.
func SlashDetailBody(def Definition, args string) string {
	return SlashAgentPrompt(def, args)
}

// SlashDetailLabel returns the detail box title for /skill:<name>.
func SlashDetailLabel(name string) string {
	return "Skill: " + strings.TrimSpace(name)
}

func writeSkillBody(b *strings.Builder, def Definition) {
	if body := strings.TrimSpace(def.Body); body != "" {
		b.WriteString(body)
		return
	}
	b.WriteString("(no instructions in SKILL.md body)")
}
