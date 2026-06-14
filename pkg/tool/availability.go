package tool

import "strings"

// ResolveName maps a model-supplied tool name to the canonical built-in name.
func ResolveName(raw string) (canonical string, known bool) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return "Tool", false
	}
	lower := strings.ToLower(trimmed)
	for _, def := range builtin {
		if strings.ToLower(def.Name) == lower {
			return def.Name, true
		}
	}
	return titleCaseToolName(trimmed), false
}

// IsProviderExposed reports whether a built-in tool should be sent to the model API.
// Requires auto-allow approval, a provider schema, and IsExecutable. See docs/tools.md.
func IsProviderExposed(name string) bool {
	def, ok := Get(name)
	if !ok {
		return false
	}
	if def.DefaultApproval != ApprovalAutoAllow {
		return false
	}
	if !IsExecutable(name) {
		return false
	}
	_, ok = providerSchema(name)
	return ok
}

// IsExecutable reports whether the agent runtime can run a built-in tool by name.
// Returns false for unknown tools. See docs/tools.md for the exposure matrix.
func IsExecutable(name string) bool {
	def, ok := Get(name)
	if !ok {
		return false
	}
	switch def.Name {
	case Read, Grep, Glob:
		return true
	default:
		return false
	}
}

func titleCaseToolName(name string) string {
	if len(name) == 1 {
		return strings.ToUpper(name)
	}
	return strings.ToUpper(name[:1]) + name[1:]
}
