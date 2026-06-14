package anthropic

import "strings"

// SanitizeModelID trims whitespace from a model identifier.
func SanitizeModelID(model string) string {
	return strings.TrimSpace(model)
}
