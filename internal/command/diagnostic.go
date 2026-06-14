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

func diagnosticListTools(ctx *Context, _ string) string {
	var b strings.Builder
	for _, def := range tool.All() {
		fmt.Fprintf(&b, "  %s (%s) — %s\n", def.Name, def.DefaultApproval, def.Description)
	}

	for _, def := range tools.Diagnostic() {
		fmt.Fprintf(&b, "  %s (%s) — %s\n", def.Name, def.DefaultApproval, def.Description)
	}

	ctx.pendingDetailLabel = "Available tools"
	ctx.pendingDetailBody = strings.TrimRight(b.String(), "\n")
	ctx.pendingDetailExpanded = true
	return ""
}

func diagnosticSystemPrompt(ctx *Context, _ string) string {
	if strings.TrimSpace(ctx.SystemPrompt) == "" {
		return fmt.Sprintf("/%s: not yet implemented", DiagnosticSystemPrompt)
	}

	ctx.pendingDetailLabel = "System prompt"
	ctx.pendingDetailBody = ctx.SystemPrompt
	return ""
}

func diagnosticOpenLog(ctx *Context, args string) string {
	args = strings.ToLower(strings.TrimSpace(args))
	if args == "" {
		return fmt.Sprintf("Usage: /%s <%s>", DiagnosticOpenLog, ArgsHint(openLogArgs))
	}

	switch args {
	case "requests":
		return displayLogFile(ctx, ctx.RequestsLogPath, "Requests log", requestsLogEmptyMessage)
	case "thinking_delta":
		return displayFilteredLog(ctx, ctx.RequestsLogPath, "thinking_delta")
	case "system", "thinking", "ai":
		return displayFilteredLog(ctx, ctx.LogPath, args)
	default:
		return fmt.Sprintf("/%s: unknown log %q — use %s", DiagnosticOpenLog, args, ArgsHint(openLogArgs))
	}
}

func setOpenLogDetail(ctx *Context, label, path, body string) {
	var b strings.Builder
	if strings.TrimSpace(path) != "" {
		b.WriteString(path)
		b.WriteString("\n\n")
	}
	b.WriteString(body)
	ctx.pendingDetailLabel = label
	ctx.pendingDetailBody = strings.TrimRight(b.String(), "\n")
	ctx.pendingDetailExpanded = true
}

func requestsLogEmptyMessage(path string) string {
	return fmt.Sprintf(
		"Requests log (%s) is empty. Send a prompt to the agent first, or use /%s thinking_delta after a turn with thinking enabled.",
		path, DiagnosticOpenLog,
	)
}

func displayLogFile(ctx *Context, path, label string, missing func(string) string) string {
	if path == "" {
		return fmt.Sprintf("/%s: %s not available", DiagnosticOpenLog, strings.ToLower(label))
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
		setOpenLogDetail(ctx, label, path, fmt.Sprintf("(%s) is empty.", label))
		return ""
	}

	setOpenLogDetail(ctx, label, path, content)
	return ""
}

func displayFilteredLog(ctx *Context, path, kind string) string {
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

	label := fmt.Sprintf("Session log (%s)", kind)
	if kind == "thinking_delta" {
		label = "Requests log (thinking_delta)"
	}

	content = strings.TrimSpace(content)
	if content == "" {
		var msg string
		switch kind {
		case "thinking":
			msg = fmt.Sprintf("Session log (%s) has no [thinking] entries yet. Run an agent turn with thinking enabled (T: high), then retry.", path)
		case "thinking_delta":
			msg = fmt.Sprintf("Requests log (%s) has no [thinking_delta] entries yet. Run an agent turn with thinking enabled (T: high), then retry.", path)
		case "ai":
			msg = fmt.Sprintf("Session log (%s) has no [ai] entries yet. Send a prompt to the agent first.", path)
		default:
			msg = fmt.Sprintf("Log (%s) has no [%s] entries.", path, kind)
		}
		setOpenLogDetail(ctx, label, path, msg)
		return ""
	}
	setOpenLogDetail(ctx, label, path, content)
	return ""
}

func diagnosticDebug(*Context, string) string {
	return notImplemented(DiagnosticDebug)(nil, "")
}
