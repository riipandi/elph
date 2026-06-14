package tools

import "strings"

var legacyDiagnosticNames = map[string]string{
	"diagnostic_list_tools":    DiagnosticListTools,
	"diagnostic_system_prompt": DiagnosticSystemPrompt,
	"diagnostic_open_log":      DiagnosticOpenLog,
}

// ResolveName maps a raw tool name to a canonical diagnostic tool name.
func ResolveName(raw string) (canonical string, known bool) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return "", false
	}
	lower := strings.ToLower(trimmed)
	if name, ok := legacyDiagnosticNames[lower]; ok {
		return name, true
	}
	for _, def := range diagnostic {
		if strings.ToLower(def.Name) == lower {
			return def.Name, true
		}
	}
	return trimmed, false
}
