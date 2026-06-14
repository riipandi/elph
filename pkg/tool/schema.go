package tool

import "github.com/riipandi/elph/pkg/ai/provider"

// ProviderDefinitions returns built-in tools as provider-native schemas for the model API.
// Results are filtered by IsProviderExposed; see docs/tools.md § Provider API exposure.
func ProviderDefinitions() []provider.ToolDefinition {
	return FilterProviderTools(collectBuiltinProviderDefinitions())
}

// FilterProviderTools keeps only definitions that should be sent to the model API
// (auto-allow, executable, and with a provider schema). See docs/tools.md.
func FilterProviderTools(tools []provider.ToolDefinition) []provider.ToolDefinition {
	if len(tools) == 0 {
		return nil
	}
	out := make([]provider.ToolDefinition, 0, len(tools))
	for _, def := range tools {
		if IsProviderExposed(def.Name) {
			out = append(out, def)
		}
	}
	return out
}

func collectBuiltinProviderDefinitions() []provider.ToolDefinition {
	out := make([]provider.ToolDefinition, 0, len(builtin))
	for _, def := range builtin {
		schema, ok := providerSchema(def.Name)
		if !ok {
			continue
		}
		out = append(out, provider.ToolDefinition{
			Name:        def.Name,
			Description: def.Description,
			Parameters:  schema,
		})
	}
	return out
}

func providerSchema(name string) (map[string]any, bool) {
	switch name {
	case Read:
		return objectSchema(map[string]propertySpec{
			"path": {typ: "string", description: "Absolute or workspace-relative file path"},
		}, "path"), true
	case Write:
		return objectSchema(map[string]propertySpec{
			"path":     {typ: "string", description: "Absolute or workspace-relative file path"},
			"contents": {typ: "string", description: "Full file contents to write"},
		}, "path", "contents"), true
	case Edit:
		return objectSchema(map[string]propertySpec{
			"path":        {typ: "string", description: "Absolute or workspace-relative file path"},
			"old_string":  {typ: "string", description: "Exact text to replace"},
			"new_string":  {typ: "string", description: "Replacement text"},
			"replace_all": {typ: "boolean", description: "Replace every occurrence (default false)"},
		}, "path", "old_string", "new_string"), true
	case Grep:
		return objectSchema(map[string]propertySpec{
			"pattern":     {typ: "string", description: "Regular expression to search for"},
			"path":        {typ: "string", description: "Directory or file to search in"},
			"glob":        {typ: "string", description: "Optional glob filter"},
			"output_mode": {typ: "string", description: "content, files_with_matches, or count"},
		}, "pattern"), true
	case Glob:
		return objectSchema(map[string]propertySpec{
			"pattern": {typ: "string", description: "Glob pattern, e.g. **/*.go"},
			"path":    {typ: "string", description: "Directory to search in"},
		}, "pattern"), true
	case FetchURL:
		return objectSchema(map[string]propertySpec{
			"url": {typ: "string", description: "URL to fetch"},
		}, "url"), true
	case WebSearch:
		return objectSchema(map[string]propertySpec{
			"query": {typ: "string", description: "Search query"},
		}, "query"), true
	case CodeSearch:
		return objectSchema(map[string]propertySpec{
			"query": {typ: "string", description: "Code search query"},
		}, "query"), true
	case ReadMediaFile:
		return objectSchema(map[string]propertySpec{
			"path": {typ: "string", description: "Path to an image or video file"},
		}, "path"), true
	case Bash:
		return objectSchema(map[string]propertySpec{
			"command":     {typ: "string", description: "Shell command to execute via bash -c in the workspace directory"},
			"description": {typ: "string", description: "Short description of what the command does"},
		}, "command"), true
	case AskUser:
		return askUserSchema(), true
	case EnterPlanMode, ExitPlanMode:
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
				"description": "Optional multiple-choice answers; omit for free-text input",
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
