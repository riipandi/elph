package schema

import (
	"github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/riipandi/elph/pkg/tools/catalog"
	"github.com/riipandi/elph/pkg/tools/exposure"
)

// ProviderDefinitions returns built-in tools as provider-native schemas for the model API.
// Results are filtered by IsProviderExposed; see docs/tools.md § Provider API exposure.
func ProviderDefinitions() []protocol.ToolDefinition {
	return FilterProviderTools(collectBuiltinProviderDefinitions())
}

// FilterProviderTools keeps only definitions that should be sent to the model API
// (auto-allow, executable, and with a provider schema). See docs/tools.md.
func FilterProviderTools(tools []protocol.ToolDefinition) []protocol.ToolDefinition {
	if len(tools) == 0 {
		return nil
	}
	out := make([]protocol.ToolDefinition, 0, len(tools))
	for _, def := range tools {
		if IsProviderExposed(def.Name) {
			out = append(out, def)
		}
	}
	return out
}

// IsProviderExposed reports whether a built-in tool should be sent to the model API.
func IsProviderExposed(name string) bool {
	def, ok := catalog.Get(name)
	if !ok {
		return false
	}
	if def.DefaultApproval != catalog.ApprovalAutoAllow && def.DefaultApproval != catalog.ApprovalRequiresApproval {
		return false
	}
	if !exposure.IsExecutable(name) {
		return false
	}
	_, ok = ProviderSchema(name)
	return ok
}

func collectBuiltinProviderDefinitions() []protocol.ToolDefinition {
	out := make([]protocol.ToolDefinition, 0, len(catalog.All()))
	for _, def := range catalog.All() {
		s, ok := ProviderSchema(def.Name)
		if !ok {
			continue
		}
		out = append(out, protocol.ToolDefinition{
			Name:        def.Name,
			Description: def.Description,
			Parameters:  s,
		})
	}
	return out
}

// ProviderSchema returns the JSON Schema for a built-in tool name.
func ProviderSchema(name string) (map[string]any, bool) {
	switch name {
	case catalog.Read:
		return objectSchema(map[string]propertySpec{
			"path":        {typ: "string", description: "Absolute or workspace-relative file path"},
			"line_offset": {typ: "integer", description: "Starting line number (1-indexed). Negative values read from end of file (tail). Omit to start at line 1."},
			"n_lines":     {typ: "integer", description: "Number of lines to read. Omit to read up to the internal cap (1000 lines / 100 KB)."},
		}, "path"), true
	case catalog.Write:
		return objectSchema(map[string]propertySpec{
			"path":     {typ: "string", description: "Absolute or workspace-relative file path"},
			"contents": {typ: "string", description: "Full file contents to write"},
			"mode":     {typ: "string", description: "Write mode: \"overwrite\" (default) or \"append\". append adds content to the end without adding a newline."},
		}, "path", "contents"), true
	case catalog.Edit:
		return objectSchema(map[string]propertySpec{
			"path":        {typ: "string", description: "Absolute or workspace-relative file path"},
			"old_string":  {typ: "string", description: "Exact text to replace"},
			"new_string":  {typ: "string", description: "Replacement text"},
			"replace_all": {typ: "boolean", description: "Replace every occurrence (default false)"},
		}, "path", "old_string", "new_string"), true
	case catalog.Grep:
		return objectSchema(map[string]propertySpec{
			"pattern":       {typ: "string", description: "Regular expression to search for"},
			"path":          {typ: "string", description: "Directory or file to search in"},
			"glob":          {typ: "string", description: "Optional glob filter"},
			"output_mode":   {typ: "string", description: "content, files_with_matches, or count"},
			"context_lines": {typ: "integer", description: "Number of context lines to show before and each match"},
		}, "pattern"), true
	case catalog.Glob:
		return objectSchema(map[string]propertySpec{
			"pattern": {typ: "string", description: "Glob pattern, e.g. **/*.go"},
			"path":    {typ: "string", description: "Directory to search in"},
		}, "pattern"), true
	case catalog.FetchURL:
		return objectSchema(map[string]propertySpec{
			"url": {typ: "string", description: "URL to fetch"},
		}, "url"), true
	case catalog.WebSearch:
		return objectSchema(map[string]propertySpec{
			"query": {
				typ:         "string",
				description: "Search query",
			},
			"engine": {
				typ:         "string",
				description: "Optional engine: duckduckgo, jina, brave, serpapi, tavily, firecrawl, perplexity, exa. Auto-selects the best configured engine when omitted; DuckDuckGo is the fallback.",
			},
			"limit": {
				typ:         "integer",
				description: "Number of results to return (1-20, default: 5).",
			},
			"include_content": {
				typ:         "boolean",
				description: "Whether to include the content of each result page. Can consume many tokens when true.",
			},
		}, "query"), true
	case catalog.CodeSearch:
		return objectSchema(map[string]propertySpec{
			"query": {
				typ:         "string",
				description: "Code search query",
			},
			"provider": {
				typ:         "string",
				description: "Optional provider: github, gitlab. Auto mode always searches GitHub; also searches GitLab when GITLAB_TOKEN is set. GITHUB_PERSONAL_ACCESS_TOKEN is optional for GitHub.",
			},
		}, "query"), true
	case catalog.ReadMediaFile:
		return objectSchema(map[string]propertySpec{
			"path": {typ: "string", description: "Path to an image or video file"},
		}, "path"), true
	case catalog.Bash:
		return objectSchema(map[string]propertySpec{
			"command":     {typ: "string", description: "Shell command to execute"},
			"description": {typ: "string", description: "Short description of what the command does"},
			"cwd":         {typ: "string", description: "Working directory for the command. Omit to use the session's working directory."},
			"timeout":     {typ: "integer", description: "Timeout in seconds. Default: 120s, max: 300s."},
		}, "command"), true
	case catalog.AskUser:
		return askUserSchema(), true
	case catalog.Skill:
		return objectSchema(map[string]propertySpec{
			"skill": {
				typ:         "string",
				description: "Skill name (agentskills.io SKILL.md name field)",
			},
			"args": {
				typ:         "string",
				description: "Optional additional argument text appended to the skill instructions",
			},
		}, "skill"), true
	case catalog.TodoList:
		return todoListSchema(), true
	case catalog.EnterPlanMode, catalog.ExitPlanMode:
		return objectSchema(map[string]propertySpec{
			"reason": {typ: "string", description: "Short reason for the mode change"},
		}, "reason"), true
	default:
		return nil, false
	}
}

type propertySpec struct {
	typ         string
	description string
}

func todoListSchema() map[string]any {
	return map[string]any{
		"type": "object",
		"properties": map[string]any{
			"todos": map[string]any{
				"type":        "array",
				"description": "Task list to set. Omit to query the current list; pass an empty array to clear.",
				"items": map[string]any{
					"type": "object",
					"properties": map[string]any{
						"title": map[string]any{
							"type":        "string",
							"description": "Task description",
						},
						"status": map[string]any{
							"type":        "string",
							"description": "pending, in_progress, or done",
							"enum":        []string{"pending", "in_progress", "done"},
						},
					},
					"required": []string{"title", "status"},
				},
			},
		},
	}
}

func askUserSchema() map[string]any {
	return map[string]any{
		"type": "object",
		"properties": map[string]any{
			"question": map[string]any{
				"type":        "string",
				"description": "Question to ask the user",
			},
			"options": map[string]any{
				"type":        "array",
				"items":       map[string]any{"type": "string"},
				"description": "Suggested answers shown as quick picks; the user can still type a custom answer unless allowCustom is false",
			},
			"allowCustom": map[string]any{
				"type":        "boolean",
				"description": "When options are provided, also allow a free-text answer (default true)",
			},
		},
		"required": []string{"question"},
	}
}

func objectSchema(props map[string]propertySpec, required ...string) map[string]any {
	properties := make(map[string]any, len(props))
	for name, spec := range props {
		properties[name] = map[string]any{
			"type":        spec.typ,
			"description": spec.description,
		}
	}
	return map[string]any{
		"type":       "object",
		"properties": properties,
		"required":   required,
	}
}
