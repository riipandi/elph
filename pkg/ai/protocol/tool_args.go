package protocol

import (
	"encoding/json"
	"strings"
)

// NormalizeToolArguments returns valid JSON object bytes for provider tool calls.
// Empty, null, or invalid JSON is coerced to "{}".
func NormalizeToolArguments(raw json.RawMessage) json.RawMessage {
	trimmed := strings.TrimSpace(string(raw))
	if trimmed == "" || trimmed == "null" {
		return json.RawMessage("{}")
	}
	if json.Valid(raw) {
		return raw
	}
	return json.RawMessage("{}")
}