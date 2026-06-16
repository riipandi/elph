package agent

import (
	"fmt"
	"strings"
	"time"

	"github.com/riipandi/elph/pkg/ai/protocol"
)

// CompactionReason describes why compaction was triggered.
type CompactionReason string

const (
	ReasonManual    CompactionReason = "manual"    // User ran /compact
	ReasonThreshold CompactionReason = "threshold" // Proactive: approaching context limit
	ReasonOverflow  CompactionReason = "overflow"  // Reactive: context-limit error from provider
)

// CompactionEntry stores metadata about a compaction event (like Pi's CompactionEntry).
// This persists across compactions and tracks what was summarized.
type CompactionEntry struct {
	Summary         string           `json:"summary"`          // Structured summary of compacted messages
	TokensBefore    int              `json:"tokens_before"`    // Context tokens before compaction
	Timestamp       time.Time        `json:"timestamp"`        // When compaction occurred
	Reason          CompactionReason `json:"reason"`           // Why compaction was triggered
	MessagesRemoved int              `json:"messages_removed"` // Number of messages removed
	ReadFiles       []string         `json:"read_files"`       // Files read during compacted turns
	ModifiedFiles   []string         `json:"modified_files"`   // Files modified during compacted turns
}

// CompactionResult is returned by compaction functions with metadata.
type CompactionResult struct {
	Messages []protocol.ChatMessage `json:"messages"` // Compacted messages
	Changed  bool                   `json:"changed"`  // Whether any messages were removed
	Entry    *CompactionEntry       `json:"entry"`    // Metadata (nil if no compaction occurred)
}

// EstimateTokens estimates token count from byte size (rough: 1 token ≈ 4 bytes for English).
func EstimateTokens(bytes int) int {
	return bytes / 4
}

// CompactionThreshold defines when compaction is worthwhile.
type CompactionThreshold struct {
	MinMessages  int // Minimum messages before compaction makes sense
	MinBytes     int // Minimum total bytes before compaction
	MinTokens    int // Minimum tokens before compaction
	ContextUsage int // Context usage percentage threshold (0-100)
}

// DefaultCompactionThreshold returns sensible defaults.
func DefaultCompactionThreshold() CompactionThreshold {
	return CompactionThreshold{
		MinMessages:  10,        // Need at least 10 messages to benefit
		MinBytes:     64 * 1024, // 64 KB minimum
		MinTokens:    16 * 1024, // 16K tokens minimum
		ContextUsage: 70,        // Compact when context is 70% full
	}
}

// ShouldCompact determines if compaction is worthwhile based on thresholds.
// Returns true if the conversation is large enough to benefit from compaction.
func ShouldCompact(messages []protocol.ChatMessage, threshold CompactionThreshold) bool {
	if len(messages) == 0 {
		return false
	}

	// Check message count
	if len(messages) < threshold.MinMessages {
		return false
	}

	// Check total bytes
	totalBytes := historyUTF8Size(messages)
	if totalBytes < threshold.MinBytes {
		return false
	}

	// Check token estimate
	tokens := EstimateTokens(totalBytes)
	if tokens < threshold.MinTokens {
		return false
	}

	return true
}

// ShouldAutoCompact checks if auto-compaction should trigger based on context usage.
// contextWindow is the model's context window size, currentTokens is current usage.
func ShouldAutoCompact(contextWindow, currentTokens int, threshold CompactionThreshold) bool {
	if contextWindow <= 0 || currentTokens <= 0 {
		return false
	}

	usagePercent := (currentTokens * 100) / contextWindow
	return usagePercent >= threshold.ContextUsage
}

// extractFileOps extracts file operations from tool calls in messages.
func extractFileOps(messages []protocol.ChatMessage) (readFiles, modifiedFiles []string) {
	seen := make(map[string]bool)
	for _, msg := range messages {
		if msg.Role != "assistant" {
			continue
		}
		for _, call := range msg.ToolCalls {
			name := strings.ToLower(call.Name)
			// Extract file path from arguments (simplified - real impl would parse JSON)
			args := string(call.Arguments)
			if name == "read" || name == "cat" {
				if path := extractPathFromArgs(args); path != "" && !seen[path] {
					readFiles = append(readFiles, path)
					seen[path] = true
				}
			} else if name == "write" || name == "edit" {
				if path := extractPathFromArgs(args); path != "" && !seen[path] {
					modifiedFiles = append(modifiedFiles, path)
					seen[path] = true
				}
			}
		}
	}
	return
}

// extractPathFromArgs extracts a file path from tool call arguments.
// This is a simplified version - real implementation would parse JSON properly.
func extractPathFromArgs(args string) string {
	// Look for "path": "value" or "file_path": "value"
	for _, key := range []string{"path", "file_path", "filePath"} {
		idx := strings.Index(args, `"`+key+`"`)
		if idx == -1 {
			continue
		}
		rest := args[idx+len(key)+3:] // skip past key and quotes
		// Find the value
		start := strings.Index(rest, `"`)
		if start == -1 {
			continue
		}
		rest = rest[start+1:]
		end := strings.Index(rest, `"`)
		if end == -1 {
			continue
		}
		return rest[:end]
	}
	return ""
}

// GenerateCompactionSummary creates a structured summary in Pi's format.
// This would normally call an LLM, but for now we generate a simple summary.
func GenerateCompactionSummary(
	messages []protocol.ChatMessage,
	previousSummary string,
	reason CompactionReason,
) string {
	var b strings.Builder

	// Count by role
	userMsgs := 0
	assistantMsgs := 0
	toolMsgs := 0
	for _, msg := range messages {
		switch msg.Role {
		case "user":
			userMsgs++
		case "assistant":
			assistantMsgs++
		case "tool":
			toolMsgs++
		}
	}

	// Extract file operations
	readFiles, modifiedFiles := extractFileOps(messages)

	b.WriteString("## Compaction Summary\n\n")
	b.WriteString(fmt.Sprintf("**Reason:** %s\n", reason))
	b.WriteString(fmt.Sprintf("**Messages compacted:** %d user, %d assistant, %d tool\n\n", userMsgs, assistantMsgs, toolMsgs))

	if previousSummary != "" {
		b.WriteString("## Previous Context\n")
		b.WriteString(previousSummary)
		b.WriteString("\n\n")
	}

	// File tracking (Pi-style cumulative tracking)
	if len(readFiles) > 0 || len(modifiedFiles) > 0 {
		if len(readFiles) > 0 {
			b.WriteString("<read-files>\n")
			for _, f := range readFiles {
				b.WriteString(f + "\n")
			}
			b.WriteString("</read-files>\n")
		}
		if len(modifiedFiles) > 0 {
			b.WriteString("<modified-files>\n")
			for _, f := range modifiedFiles {
				b.WriteString(f + "\n")
			}
			b.WriteString("</modified-files>\n")
		}
	}

	return b.String()
}

// CompactMessagesWithEntry compacts messages and returns a CompactionEntry.
// This is the main entry point for compaction with full metadata tracking.
func CompactMessagesWithEntry(
	messages []protocol.ChatMessage,
	ratio int,
	reason CompactionReason,
	tokensBefore int,
) CompactionResult {
	if len(messages) == 0 {
		return CompactionResult{
			Messages: messages,
			Changed:  false,
			Entry:    nil,
		}
	}

	// Smart compaction: skip if conversation is too small
	threshold := DefaultCompactionThreshold()
	if !ShouldCompact(messages, threshold) {
		return CompactionResult{
			Messages: messages,
			Changed:  false,
			Entry:    nil,
		}
	}

	beforeCount := len(messages)
	var compacted []protocol.ChatMessage

	if ratio > 0 && ratio < 100 {
		compacted = CompactMessagesToRatio(messages, ratio)
	} else {
		compacted = CompactMessages(messages)
	}

	changed := len(compacted) < beforeCount
	if !changed {
		return CompactionResult{
			Messages: messages,
			Changed:  false,
			Entry:    nil,
		}
	}

	// Generate summary
	summary := GenerateCompactionSummary(
		messages[:beforeCount-len(compacted)],
		"", // No previous summary for basic compaction
		reason,
	)

	entry := &CompactionEntry{
		Summary:         summary,
		TokensBefore:    tokensBefore,
		Timestamp:       time.Now(),
		Reason:          reason,
		MessagesRemoved: beforeCount - len(compacted),
	}

	// Track file operations
	readFiles, modifiedFiles := extractFileOps(messages[:beforeCount-len(compacted)])
	entry.ReadFiles = readFiles
	entry.ModifiedFiles = modifiedFiles

	return CompactionResult{
		Messages: compacted,
		Changed:  true,
		Entry:    entry,
	}
}
