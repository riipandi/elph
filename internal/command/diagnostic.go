package command

import (
	"fmt"
	"os"
	"strings"

	"github.com/riipandi/elph/internal/runtime"
	"github.com/riipandi/elph/internal/tools"
	"github.com/riipandi/elph/pkg/tool"
)

const (
	DiagnosticListTools    = "diagnostic:list-tools"
	DiagnosticSystemPrompt = "diagnostic:system-prompt"
	DiagnosticOpenLog      = "diagnostic:open-log"
	DiagnosticDebug        = "diagnostic:debug"
)

var openLogArgs = []ArgChoice{
	{Value: "system", Description: "Session events, notices, and command output"},
	{Value: "thinking", Description: "Reasoning output recorded at end of agent turns"},
	{Value: "thinking_delta", Description: "Reserved — use thinking after a completed turn"},
	{Value: "ai", Description: "Assistant responses recorded at end of agent turns"},
	{Value: "requests", Description: "Provider steps, tool runs, and stream trace"},
}

func diagnosticListTools(*Context, string) string {
	var b strings.Builder
	b.WriteString("Available tools:\n")

	for _, def := range tool.All() {
		fmt.Fprintf(&b, "  %s (%s) — %s\n", def.Name, def.DefaultApproval, def.Description)
	}

	for _, def := range tools.Diagnostic() {
		fmt.Fprintf(&b, "  %s (%s) — %s\n", def.Name, def.DefaultApproval, def.Description)
	}

	return strings.TrimRight(b.String(), "\n")
}

func diagnosticSystemPrompt(ctx *Context, _ string) string {
	if strings.TrimSpace(ctx.SystemPrompt) == "" {
		return fmt.Sprintf("/%s: not yet implemented", DiagnosticSystemPrompt)
	}

	var b strings.Builder
	b.WriteString("System prompt:\n\n")
	b.WriteString(ctx.SystemPrompt)
	return b.String()
}

func diagnosticOpenLog(ctx *Context, args string) string {
	args = strings.ToLower(strings.TrimSpace(args))
	if args == "" {
		return fmt.Sprintf("Usage: /%s <%s>", DiagnosticOpenLog, ArgsHint(openLogArgs))
	}

	switch args {
	case "requests":
		return displayLogFile(ctx.RequestsLogPath, "requests", requestsLogEmptyMessage)
	case "thinking_delta":
		return displayFilteredLog(ctx.RequestsLogPath, "thinking_delta")
	case "system", "thinking", "ai":
		return displayFilteredLog(ctx.LogPath, args)
	default:
		return fmt.Sprintf("/%s: unknown log %q — use %s", DiagnosticOpenLog, args, ArgsHint(openLogArgs))
	}
}

func requestsLogEmptyMessage(path string) string {
	return fmt.Sprintf(
		"Requests log (%s) is empty. Send a prompt to the agent first, or use /%s thinking_delta after a turn with thinking enabled.",
		path, DiagnosticOpenLog,
	)
}

func displayLogFile(path, label string, missing func(string) string) string {
	if path == "" {
		return fmt.Sprintf("/%s: %s log not available", DiagnosticOpenLog, label)
	}

	content, err := runtime.ReadLogTail(path, 0)
	if err != nil {
		if os.IsNotExist(err) && missing != nil {
			return missing(path)
		}
		return fmt.Sprintf("/%s: %v", DiagnosticOpenLog, err)
	}

	content = strings.TrimSpace(content)
	if content == "" {
		return fmt.Sprintf("%s log (%s) is empty.", label, path)
	}

	var b strings.Builder
	fmt.Fprintf(&b, "%s log: %s\n\n", label, path)
	b.WriteString(content)
	return b.String()
}

func displayFilteredLog(path, kind string) string {
	if path == "" {
		return fmt.Sprintf("/%s: session log not available", DiagnosticOpenLog)
	}

	content, err := runtime.FilterLogByKind(path, kind, 0)
	if err != nil {
		if os.IsNotExist(err) {
			return fmt.Sprintf("Session log (%s) has not been created yet — send a message to the agent first.", path)
		}
		return fmt.Sprintf("/%s: %v", DiagnosticOpenLog, err)
	}

	content = strings.TrimSpace(content)
	if content == "" {
		switch kind {
		case "thinking":
			return fmt.Sprintf("Session log (%s) has no [thinking] entries yet. Run an agent turn with thinking enabled (T: high), then retry.", path)
		case "thinking_delta":
			return fmt.Sprintf("Requests log (%s) has no [thinking_delta] entries yet. Run an agent turn with thinking enabled (T: high), then retry.", path)
		case "ai":
			return fmt.Sprintf("Session log (%s) has no [ai] entries yet. Send a prompt to the agent first.", path)
		default:
			return fmt.Sprintf("Log (%s) has no [%s] entries.", path, kind)
		}
	}

	label := "Session log"
	if kind == "thinking_delta" {
		label = "Requests log"
	}
	var b strings.Builder
	fmt.Fprintf(&b, "%s (%s) — [%s]:\n\n", label, path, kind)
	b.WriteString(content)
	return b.String()
}

func diagnosticDebug(*Context, string) string {
	return notImplemented(DiagnosticDebug)(nil, "")
}
