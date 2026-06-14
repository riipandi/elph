package tools

import "strings"

// ResolveName maps a raw tool name to a canonical diagnostic tool name.
func ResolveName(raw string) (canonical string, known bool) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return "", false
	}
	lower := strings.ToLower(trimmed)
	for _, def := range diagnostic {
		if strings.ToLower(def.Name) == lower {
			return def.Name, true
		}
	}
	return trimmed, false
}
