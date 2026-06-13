package prompt

import (
	"strings"

	"github.com/riipandi/elph/pkg/tool"
)

// Options configures how the system prompt is assembled.
type Options struct {
	// WorkDir is used to discover AGENTS.md by walking up the directory tree.
	WorkDir string

	// Tools lists tools to inject. When nil, every built-in tool is included.
	Tools []tool.Definition

	// AdditionalInstructions are appended after project context.
	AdditionalInstructions string
}

// Build assembles the system prompt:
//  1. embedded base template (template/system.md)
//  2. dynamic available-tools section from pkg/tool and internal/tools
//  3. nearest AGENTS.md context
//  4. additional user instructions
func Build(opts Options) string {
	sections := []string{strings.TrimSpace(baseSystemPrompt)}

	if toolsSection := formatToolsSection(catalogEntries(opts.Tools)); toolsSection != "" {
		sections = append(sections, toolsSection)
	}

	if content, path, ok := FindAgentsMD(opts.WorkDir); ok {
		sections = append(sections, formatAgentsSection(content, path))
	}

	if extra := strings.TrimSpace(opts.AdditionalInstructions); extra != "" {
		sections = append(sections, "## Additional Instructions\n\n"+extra)
	}

	return strings.Join(sections, "\n\n")
}

func formatAgentsSection(content, path string) string {
	var b strings.Builder
	b.WriteString("## Project Instructions\n\n")
	b.WriteString("The following instructions come from `")
	b.WriteString(path)
	b.WriteString("`:\n\n")
	b.WriteString(strings.TrimSpace(content))
	return b.String()
}