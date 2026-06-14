package prompt

import (
	"fmt"
	"strings"
)

var sectionDescriptions = map[string]string{
	"File Tools":          "File tools handle reading, writing, and searching the local filesystem - the foundation for code analysis and modification tasks.",
	"State Management":    "State management tools persist structured session state across tool rounds and user turns.",
	"Collaboration Tools": "Collaboration tools handle inter-Agent coordination, user interaction, and Skill invocation.",
}

func formatAvailableTools(entries []Entry) string {
	if len(entries) == 0 {
		return ""
	}

	sections := groupBySection(entries)

	var b strings.Builder
	b.WriteString("## Available Tools")

	for _, section := range sections {
		b.WriteByte('\n')
		fmt.Fprintf(&b, "### %s", section.Name)
		if desc := sectionDescriptions[section.Name]; desc != "" {
			b.WriteByte('\n')
			b.WriteString(desc)
		}

		for _, entry := range section.Tools {
			b.WriteByte('\n')
			line := fmt.Sprintf("- %s (%s): %s", entry.Name, entry.DefaultApproval, entry.Description)
			if entry.RequiresConfirmation {
				line += " Requires user confirmation after completion."
			}
			b.WriteString(line)
		}

		if sectionHasTool(section.Tools, "ExitPlanMode") {
			b.WriteByte('\n')
			b.WriteString("`ExitPlanMode` requires user confirmation after completion.")
		}
	}

	return strings.TrimRight(b.String(), "\n")
}

func sectionHasTool(tools []Entry, name string) bool {
	for _, entry := range tools {
		if entry.Name == name {
			return true
		}
	}
	return false
}

func normalizePrompt(s string) string {
	s = strings.TrimSpace(s)

	raw := strings.Split(s, "\n")
	lines := make([]string, 0, len(raw))
	for _, line := range raw {
		lines = append(lines, strings.TrimRight(line, " \t"))
	}

	out := make([]string, 0, len(lines))
	for _, line := range lines {
		if line == "" {
			if len(out) == 0 || out[len(out)-1] == "" {
				continue
			}
			out = append(out, "")
			continue
		}

		if isHeading(line) && len(out) > 0 && out[len(out)-1] != "" {
			out = append(out, "")
		}

		out = append(out, line)
	}

	return strings.Join(out, "\n")
}

func isHeading(line string) bool {
	return strings.HasPrefix(strings.TrimSpace(line), "#")
}
