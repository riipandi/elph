package agent

import (
	"strings"
	"unicode/utf8"

	"github.com/riipandi/elph/pkg/ai/protocol"
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

func messageUTF8Size(msg protocol.ChatMessage) int {
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

func historyUTF8Size(messages []protocol.ChatMessage) int {
	total := 0
	for _, msg := range messages {
		total += messageUTF8Size(msg)
	}
	return total
}

func truncateHistoryMessage(msg protocol.ChatMessage) protocol.ChatMessage {
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
// It ensures tool messages are never orphaned: if the first message is not a user,
// it skips all non-user messages until the first user turn, so paired
// assistant(tool_calls) → tool responses are removed as a unit.
func removeOldestTurn(messages []protocol.ChatMessage) []protocol.ChatMessage {
	if len(messages) == 0 {
		return nil
	}

	// When the first message is not a user (e.g. after prior compaction removed users),
	// skip all non-user messages to find the first user turn boundary. This prevents
	// orphaning tool messages by removing just their preceding assistant(tool_calls).
	start := 0
	for start < len(messages) && messages[start].Role != "user" {
		start++
	}
	if start >= len(messages) {
		return nil
	}

	i := start + 1
	for i < len(messages) && messages[i].Role != "user" {
		i++
	}
	return messages[i:]
}

// CompactMessages trims large payloads and drops oldest turns to stay within limits.
func CompactMessages(messages []protocol.ChatMessage) []protocol.ChatMessage {
	if len(messages) == 0 {
		return nil
	}

	out := make([]protocol.ChatMessage, len(messages))
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
func stripHistoryImages(messages []protocol.ChatMessage) []protocol.ChatMessage {
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

// CompactMessagesForContext aggressively reduces history when the provider
// reports a context-limit error. Returns the compacted messages and whether
// anything was removed.
func CompactMessagesForContext(messages []protocol.ChatMessage, attempt int, ratio int) ([]protocol.ChatMessage, bool) {
	if len(messages) == 0 {
		return messages, false
	}

	// Start with standard compaction.
	out := CompactMessages(messages)

	var minMessages, minBytes int
	var factor int
	if ratio > 0 && ratio < 100 {
		// Use explicit ratio (e.g., 50 = 50% of max).
		minMessages = MaxHistoryMessages * ratio / 100
		minBytes = MaxHistoryBytes * ratio / 100
		factor = 100 / ratio // for proportional tool truncation
	} else {
		// Scale limits by attempt: each retry doubles aggressiveness.
		factor = 1 << (attempt + 1) // 2, 4, 8, ...
		minMessages = MaxHistoryMessages / factor
		minBytes = MaxHistoryBytes / factor
	}
	if minMessages < 4 {
		minMessages = 4
	}
	if minBytes < 16<<10 {
		minBytes = 16 << 10 // 16KB floor
	}

	// Drop oldest turns while exceeding scaled limits.
	changed := false
	for historyUTF8Size(out) > minBytes || len(out) > minMessages {
		next := removeOldestTurn(out)
		if len(next) >= len(out) {
			break
		}
		out = next
		changed = true
		if len(out) == 0 {
			break
		}
	}

	out = stripHistoryImages(out)

	// More aggressive tool-result truncation.
	var toolLimit int
	if ratio > 0 && ratio < 100 {
		toolLimit = MaxProviderToolBytes * ratio / 100
	} else {
		truncateFactor := factor * 2
		toolLimit = MaxProviderToolBytes / truncateFactor
	}
	if toolLimit < 4<<10 {
		toolLimit = 4 << 10 // 4KB floor for tool results
	}
	for i, msg := range out {
		if msg.Role == "tool" && len(msg.Content) > toolLimit {
			out[i].Content = TruncateWithNotice(msg.Content, toolLimit)
			changed = true
		}
	}

	return out, changed
}

// CompactMessagesToRatio compacts history to a target percentage of the default
// history limits. ratio is 1-99, where 50 means compact to 50% of MaxHistoryBytes.
func CompactMessagesToRatio(messages []protocol.ChatMessage, ratio int) []protocol.ChatMessage {
	if ratio <= 0 || ratio >= 100 {
		return CompactMessages(messages)
	}

	out := CompactMessages(messages)
	targetBytes := MaxHistoryBytes * ratio / 100
	targetMessages := MaxHistoryMessages * ratio / 100
	if targetMessages < 4 {
		targetMessages = 4
	}
	if targetBytes < 16<<10 {
		targetBytes = 16 << 10
	}

	for historyUTF8Size(out) > targetBytes || len(out) > targetMessages {
		next := removeOldestTurn(out)
		if len(next) >= len(out) {
			break
		}
		out = next
		if len(out) == 0 {
			break
		}
	}

	return stripHistoryImages(out)
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
