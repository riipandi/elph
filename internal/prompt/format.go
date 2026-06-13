package prompt

import (
	"fmt"
	"strings"
)

func formatToolsSection(entries []catalogEntry) string {
	if len(entries) == 0 {
		return ""
	}

	sections := make([]string, 0)
	seen := make(map[string]bool)
	for _, entry := range entries {
		if seen[entry.section] {
			continue
		}
		seen[entry.section] = true
		sections = append(sections, entry.section)
	}

	bySection := make(map[string][]catalogEntry)
	for _, entry := range entries {
		bySection[entry.section] = append(bySection[entry.section], entry)
	}

	var b strings.Builder
	b.WriteString("## Available Tools\n\n")
	b.WriteString("The following tools are currently available:\n")

	for _, section := range sections {
		defs := bySection[section]
		if len(defs) == 0 {
			continue
		}

		fmt.Fprintf(&b, "\n### %s\n\n", section)

		for _, def := range defs {
			line := fmt.Sprintf("- **%s** (%s): %s", def.name, def.defaultApproval, def.description)
			if def.requiresConfirmation {
				line += " Requires user confirmation after completion."
			}
			b.WriteString(line)
			b.WriteByte('\n')
		}
	}

	return strings.TrimRight(b.String(), "\n")
}