package protocol

import (
	"encoding/json"
	"strings"
)

// NormalizeToolArguments returns valid JSON object bytes for provider tool calls.
// Empty, null, or invalid JSON is coerced to "{}" when no salvageable object exists.
func NormalizeToolArguments(raw json.RawMessage) json.RawMessage {
	trimmed := strings.TrimSpace(string(raw))
	if trimmed == "" || trimmed == "null" {
		return json.RawMessage("{}")
	}
	if json.Valid(raw) {
		return raw
	}
	if salvaged := salvageJSONObject(raw); len(salvaged) > 0 {
		return salvaged
	}
	return json.RawMessage("{}")
}

// salvageJSONObject recovers a JSON object from streamed or duplicated fragments
// such as "{}{"question":"hi"}" or trailing objects appended to broken prefixes.
func salvageJSONObject(raw json.RawMessage) json.RawMessage {
	s := strings.TrimSpace(string(raw))
	for strings.HasPrefix(s, "{}") {
		s = strings.TrimSpace(strings.TrimPrefix(s, "{}"))
	}
	if s != "" && json.Valid([]byte(s)) {
		return json.RawMessage(s)
	}
	for i := strings.LastIndex(s, "{"); i >= 0; i = strings.LastIndex(s[:i], "{") {
		candidate := strings.TrimSpace(s[i:])
		if json.Valid([]byte(candidate)) {
			return json.RawMessage(candidate)
		}
	}
	return nil
}
