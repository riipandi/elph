package command

import (
	"fmt"
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
	{Value: "requests", Description: "Provider and tool request/response log"},
	{Value: "system", Description: "Session events, notices, and command output"},
}

func diagnosticListTools(Context, string) string {
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

func diagnosticSystemPrompt(ctx Context, _ string) string {
	if strings.TrimSpace(ctx.SystemPrompt) == "" {
		return fmt.Sprintf("/%s: not yet implemented", DiagnosticSystemPrompt)
	}

	var b strings.Builder
	b.WriteString("System prompt:\n\n")
	b.WriteString(ctx.SystemPrompt)
	return b.String()
}

func diagnosticOpenLog(ctx Context, args string) string {
	args = strings.ToLower(strings.TrimSpace(args))
	if args == "" {
		return fmt.Sprintf("Usage: /%s <%s>", DiagnosticOpenLog, ArgsHint(openLogArgs))
	}

	switch args {
	case "requests":
		return displayLogFile(ctx.RequestsLogPath, "requests")
	case "system":
		return displayFilteredLog(ctx.LogPath, "system")
	default:
		return fmt.Sprintf("/%s: unknown log %q — use %s", DiagnosticOpenLog, args, ArgsHint(openLogArgs))
	}
}

func displayLogFile(path, label string) string {
	if path == "" {
		return fmt.Sprintf("/%s: %s log not available", DiagnosticOpenLog, label)
	}

	content, err := runtime.ReadLogTail(path, 0)
	if err != nil {
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
		return fmt.Sprintf("/%s: %v", DiagnosticOpenLog, err)
	}

	content = strings.TrimSpace(content)
	if content == "" {
		return fmt.Sprintf("System log (%s) has no [%s] entries.", path, kind)
	}

	var b strings.Builder
	fmt.Fprintf(&b, "System log: %s\n\n", path)
	b.WriteString(content)
	return b.String()
}

func diagnosticDebug(Context, string) string {
	return notImplemented(DiagnosticDebug)(Context{}, "")
}
