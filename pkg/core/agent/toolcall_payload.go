package agent

import (
	"strings"
	"unicode"
)

// StripExtractedPayloads removes assistant text that duplicates parsed tool parameters.
func StripExtractedPayloads(text string, calls []ParsedToolCall) string {
	if strings.TrimSpace(text) == "" || len(calls) == 0 {
		return text
	}

	clean := text
	for _, call := range calls {
		for _, value := range call.Parameters {
			clean = stripPayloadValue(clean, value)
			if strings.TrimSpace(clean) == "" {
				return ""
			}
		}
	}

	clean = strings.TrimSpace(clean)
	if clean == "" {
		return ""
	}
	if isToolPayloadEcho(clean, calls) {
		return ""
	}
	return clean
}

func stripPayloadValue(text, payload string) string {
	payload = strings.TrimSpace(payload)
	if payload == "" {
		return text
	}

	normText := normalizePayloadText(text)
	normPayload := normalizePayloadText(payload)
	if normText == "" {
		return text
	}
	if normPayload == "" {
		return text
	}

	switch {
	case normText == normPayload:
		return ""
	case strings.HasPrefix(normPayload, normText) && len(normText) >= 12:
		return ""
	case strings.Contains(normText, normPayload):
		return removePayloadLiteral(text, payload)
	}
	return text
}

func isToolPayloadEcho(text string, calls []ParsedToolCall) bool {
	norm := normalizePayloadText(text)
	if len(norm) < 12 {
		return false
	}
	for _, call := range calls {
		for _, value := range call.Parameters {
			normValue := normalizePayloadText(value)
			if normValue == "" {
				continue
			}
			if norm == normValue || strings.HasPrefix(normValue, norm) {
				return true
			}
		}
	}
	return false
}

func normalizePayloadText(text string) string {
	var b strings.Builder
	b.Grow(len(text))
	for _, r := range strings.ToLower(text) {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(r)
		} else if unicode.IsSpace(r) {
			b.WriteByte(' ')
		}
	}
	return strings.Join(strings.Fields(b.String()), " ")
}

func removePayloadLiteral(text, payload string) string {
	lowerText := strings.ToLower(text)
	lowerPayload := strings.ToLower(payload)
	if idx := strings.Index(lowerText, lowerPayload); idx >= 0 {
		return strings.TrimSpace(text[:idx] + text[idx+len(payload):])
	}
	return text
}
