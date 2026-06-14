package agent

import (
	"strings"
	"unicode/utf8"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/skill"
)

const truncateNotice = "\n\n(output truncated)"

// TruncateUTF8 shortens s to at most maxBytes UTF-8 without splitting a code point.
func TruncateUTF8(s string, maxBytes int) string {
	if maxBytes <= 0 || len(s) <= maxBytes {
		return s
	}
	cut := maxBytes
	for cut > 0 && !utf8.ValidString(s[:cut]) {
		cut--
	}
	if cut <= 0 {
		return ""
	}
	return s[:cut]
}

// TruncateWithNotice shortens s and appends a truncation marker when clipped.
func TruncateWithNotice(s string, maxBytes int) string {
	if maxBytes <= 0 || len(s) <= maxBytes {
		return s
	}
	notice := truncateNotice
	budget := maxBytes - len(notice)
	if budget <= 0 {
		return TruncateUTF8(s, maxBytes)
	}
	return TruncateUTF8(s, budget) + notice
}

// LimitToolRunResult returns a copy with Output bounded for UI/event use.
func LimitToolRunResult(result ToolRunResult, maxBytes int) ToolRunResult {
	if maxBytes <= 0 || len(result.Output) <= maxBytes {
		return result
	}
	out := result
	out.Output = TruncateWithNotice(result.Output, maxBytes)
	return out
}

func messageUTF8Size(msg provider.ChatMessage) int {
	n := len(msg.Content)
	for _, img := range msg.Images {
		n += len(img.Data) + len(img.MIME)
	}
	for _, call := range msg.ToolCalls {
		n += len(call.Name) + len(call.ID) + len(call.Arguments)
	}
	n += len(msg.ToolCallID) + len(msg.Role)
	return n
}

func historyUTF8Size(messages []provider.ChatMessage) int {
	total := 0
	for _, msg := range messages {
		total += messageUTF8Size(msg)
	}
	return total
}

func truncateHistoryMessage(msg provider.ChatMessage) provider.ChatMessage {
	switch msg.Role {
	case "tool":
		limit := MaxProviderToolBytes
		if skill.IsActivationContent(msg.Content) {
			limit = MaxProviderToolBytes * 4
		}
		msg.Content = TruncateWithNotice(msg.Content, limit)
	case "assistant":
		if len(msg.Content) > MaxAssistantHistoryBytes {
			msg.Content = TruncateWithNotice(msg.Content, MaxAssistantHistoryBytes)
		}
	}
	return msg
}

// removeOldestTurn drops the first user turn (user + following assistant/tool messages).
func removeOldestTurn(messages []provider.ChatMessage) []provider.ChatMessage {
	if len(messages) == 0 {
		return nil
	}
	if messages[0].Role != "user" {
		if len(messages) == 1 {
			return nil
		}
		return messages[1:]
	}

	i := 1
	for i < len(messages) && messages[i].Role != "user" {
		i++
	}
	return messages[i:]
}

// CompactMessages trims large payloads and drops oldest turns to stay within limits.
func CompactMessages(messages []provider.ChatMessage) []provider.ChatMessage {
	if len(messages) == 0 {
		return nil
	}

	out := make([]provider.ChatMessage, len(messages))
	for i, msg := range messages {
		out[i] = truncateHistoryMessage(msg)
	}

	for len(out) > MaxHistoryMessages || historyUTF8Size(out) > MaxHistoryBytes {
		next := removeOldestTurn(out)
		if len(next) == len(out) {
			break
		}
		out = next
		if len(out) == 0 {
			break
		}
	}

	return stripHistoryImages(out)
}

// stripHistoryImages drops image bytes from older user turns so history stays bounded.
func stripHistoryImages(messages []provider.ChatMessage) []provider.ChatMessage {
	if len(messages) == 0 {
		return messages
	}
	lastUser := -1
	for i, msg := range messages {
		if msg.Role == "user" {
			lastUser = i
		}
	}
	for i := range messages {
		if i != lastUser && len(messages[i].Images) > 0 {
			messages[i].Images = nil
		}
	}
	return messages
}

// ToolResultMessage formats a tool result for provider follow-up messages.
func toolResultMessageLimited(result ToolRunResult) string {
	limited := LimitToolRunResult(result, MaxProviderToolBytes)
	body := formatToolResultBody(limited)
	return TruncateWithNotice(body, MaxProviderToolBytes)
}

func formatToolResultBody(result ToolRunResult) string {
	if result.Cancelled {
		if trimmed := strings.TrimSpace(result.Output); trimmed != "" {
			return trimmed + "\n(cancelled)"
		}
		return "(cancelled)"
	}
	if result.Err != nil {
		var b strings.Builder
		b.WriteString("Tool error: ")
		b.WriteString(result.Err.Error())
		if trimmed := strings.TrimSpace(result.Output); trimmed != "" {
			b.WriteString("\n")
			b.WriteString(trimmed)
		}
		return b.String()
	}
	if trimmed := strings.TrimSpace(result.Output); trimmed == "" {
		return "(no output)"
	}
	return strings.TrimRight(result.Output, "\n")
}
