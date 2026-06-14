package runtime

import (
	"errors"
	"fmt"
	"sort"
	"strings"

	inttools "github.com/riipandi/elph/internal/tools"
	"github.com/riipandi/elph/pkg/tool"
)

var (
	// ErrToolUnknown indicates the requested tool is not registered in this session.
	ErrToolUnknown = errors.New("unknown tool")
	// ErrToolUnavailable indicates a known tool cannot be executed in this session.
	ErrToolUnavailable = errors.New("tool unavailable")
)

// UnavailableReason explains why a tool request could not run.
type UnavailableReason int

const (
	UnavailableUnknown UnavailableReason = iota
	UnavailableNotExecutable
	UnavailableDiagnosticOnly
)

// ToolRequestPresentation is UI text for an unhandled tool request.
type ToolRequestPresentation struct {
	Name   string
	Body   string
	Reason UnavailableReason
}

// ToolResult captures the outcome of a built-in or MCP tool invocation.
type ToolResult struct {
	Output    string
	Err       error
	Cancelled bool
}

// FormatToolDetailBody returns collapsible detail text for a tool result.
func FormatToolDetailBody(output string, err error, cancelled bool) string {
	if cancelled {
		var b strings.Builder
		if trimmed := strings.TrimSpace(output); trimmed != "" {
			b.WriteString(trimmed)
			b.WriteByte('\n')
		}
		b.WriteString("(cancelled)")
		return b.String()
	}
	if err != nil {
		var b strings.Builder
		b.WriteString("Tool failed\n\n")
		b.WriteString(strings.TrimSpace(err.Error()))
		if trimmed := strings.TrimSpace(output); trimmed != "" {
			b.WriteString("\n\n")
			b.WriteString(trimmed)
		}
		return b.String()
	}
	if trimmed := strings.TrimSpace(output); trimmed == "" {
		return "(no output)"
	}
	return strings.TrimRight(output, "\n")
}

// FormatToolDetailBodyFromResult formats a ToolResult for the detail box.
func FormatToolDetailBodyFromResult(result ToolResult) string {
	return FormatToolDetailBody(result.Output, result.Err, result.Cancelled)
}

// ResolveToolRequest classifies a model-emitted tool request for the detail box.
func ResolveToolRequest(rawName string, params map[string]string) ToolRequestPresentation {
	if name, ok := inttools.ResolveName(rawName); ok {
		return ToolRequestPresentation{
			Name:   name,
			Reason: UnavailableDiagnosticOnly,
			Body:   formatUnavailableToolBody(diagnosticUnavailableMessage(name), params),
		}
	}

	name, known := tool.ResolveName(rawName)
	if !known {
		return ToolRequestPresentation{
			Name:   name,
			Reason: UnavailableUnknown,
			Body:   formatUnavailableToolBody(unknownToolMessage(name), params),
		}
	}

	if tool.IsExecutable(name) {
		if tool.RequiresApproval(name) {
			return ToolRequestPresentation{
				Name:   name,
				Reason: UnavailableNotExecutable,
				Body:   formatUnavailableToolBody(requiresApprovalToolMessage(name), params),
			}
		}
		return ToolRequestPresentation{
			Name:   name,
			Reason: UnavailableNotExecutable,
			Body:   formatUnavailableToolBody(executableToolMessage(name), params),
		}
	}

	return ToolRequestPresentation{
		Name:   name,
		Reason: UnavailableNotExecutable,
		Body:   formatUnavailableToolBody(notExecutableToolMessage(name), params),
	}
}

func unknownToolMessage(name string) string {
	return fmt.Sprintf("Tool not available\n\n%q is not registered in this session.", name)
}

func notExecutableToolMessage(name string) string {
	return fmt.Sprintf(
		"Tool unavailable\n\n%s is listed for this agent but cannot be executed in this session yet.",
		name,
	)
}

func requiresApprovalToolMessage(name string) string {
	return fmt.Sprintf(
		"Tool requires approval\n\n%s can run in this session but needs user approval. Use native tool calling so the approval dialog can appear.",
		name,
	)
}

func executableToolMessage(name string) string {
	return fmt.Sprintf(
		"Tool unavailable via markup\n\n%s is executable but was requested outside the native tool loop.",
		name,
	)
}

func diagnosticUnavailableMessage(name string) string {
	hint := diagnosticSlashHint(name)
	if hint == "" {
		return fmt.Sprintf(
			"Tool unavailable\n\n%s is a diagnostic helper and cannot be run as an agent tool.",
			name,
		)
	}
	return fmt.Sprintf(
		"Tool unavailable\n\n%s is a diagnostic helper. Use the slash command %s instead.",
		name,
		hint,
	)
}

func diagnosticSlashHint(name string) string {
	switch name {
	case inttools.ListTools:
		return "/diagnostic:list-tools"
	case inttools.SystemPrompt:
		return "/diagnostic:system-prompt"
	case inttools.OpenLog:
		return "/diagnostic:open-log"
	default:
		return ""
	}
}

func formatUnavailableToolBody(summary string, params map[string]string) string {
	if len(params) == 0 {
		return strings.TrimSpace(summary)
	}

	var b strings.Builder
	b.WriteString(strings.TrimSpace(summary))
	b.WriteString("\n\nRequested parameters:")
	keys := make([]string, 0, len(params))
	for key := range params {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	for _, key := range keys {
		b.WriteString("\n")
		b.WriteString(key)
		b.WriteString(": ")
		b.WriteString(params[key])
	}
	return strings.TrimRight(b.String(), "\n")
}

// FormatToolDisplay returns log/UI text for a completed tool invocation.
func FormatToolDisplay(toolName string, result ToolResult) string {
	body := FormatToolDetailBodyFromResult(result)
	if strings.TrimSpace(toolName) == "" {
		return body
	}
	return fmt.Sprintf("%s\n\n%s", toolName, body)
}
