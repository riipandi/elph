package protocol

import "strings"

// BuildMessages returns provider messages, using explicit history when present.
func BuildMessages(req TurnRequest) []ChatMessage {
	if len(req.Messages) > 0 {
		return append([]ChatMessage(nil), req.Messages...)
	}
	out := make([]ChatMessage, 0, 1)
	if strings.TrimSpace(req.UserPrompt) != "" {
		out = append(out, ChatMessage{Role: "user", Content: req.UserPrompt})
	}
	return out
}
