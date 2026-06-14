package runtime

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"unicode/utf8"

	"github.com/riipandi/elph/pkg/tool"
	"mvdan.cc/sh/v3/syntax"
)

var ErrToolNotImplemented = errors.New("tool not implemented")

const (
	maxReadBytes = 256 << 10
	maxGrepBytes = 128 << 10
	maxGlobPaths = 500
	maxGlobBytes = 128 << 10
)

// ExecuteTool runs a built-in agent tool and returns its result.
func ExecuteTool(ctx context.Context, workDir, name string, args map[string]any) ToolResult {
	canonical, known := tool.ResolveName(name)
	if !known {
		return ToolResult{Err: ErrToolUnknown}
	}
	if !tool.IsExecutable(canonical) {
		return ToolResult{Err: ErrToolUnavailable}
	}

	switch canonical {
	case tool.Read:
		return executeRead(workDir, args)
	case tool.Grep:
		return executeGrep(ctx, workDir, args)
	case tool.Glob:
		return executeGlob(workDir, args)
	case tool.Bash:
		return executeBash(ctx, workDir, args)
	default:
		return ToolResult{Err: fmt.Errorf("%w: %s", ErrToolNotImplemented, canonical)}
	}
}

func executeRead(workDir string, args map[string]any) ToolResult {
	path, ok := stringArg(args, "path")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: path")}
	}
	full, err := resolveWorkPath(workDir, path)
	if err != nil {
		return ToolResult{Err: err}
	}
	data, err := os.ReadFile(full)
	if err != nil {
		return ToolResult{Err: err}
	}
	if len(data) > maxReadBytes {
		data = data[:maxReadBytes]
		return ToolResult{Output: string(data) + "\n\n(output truncated)"}
	}
	return ToolResult{Output: string(data)}
}

func executeGrep(ctx context.Context, workDir string, args map[string]any) ToolResult {
	pattern, ok := stringArg(args, "pattern")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: pattern")}
	}
	searchPath := workDir
	if raw, ok := stringArg(args, "path"); ok && raw != "" {
		resolved, err := resolveWorkPath(workDir, raw)
		if err != nil {
			return ToolResult{Err: err}
		}
		searchPath = resolved
	}

	cmdArgs := []string{"--regexp", pattern, "--color=never", "--line-number", "--with-filename"}
	if glob, ok := stringArg(args, "glob"); ok && glob != "" {
		cmdArgs = append(cmdArgs, "--glob", glob)
	}
	cmdArgs = append(cmdArgs, searchPath)

	cmd := exec.CommandContext(ctx, "rg", cmdArgs...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		if exit, ok := err.(*exec.ExitError); ok && exit.ExitCode() == 1 {
			return ToolResult{Output: "(no matches)"}
		}
		return ToolResult{Output: truncateToolOutput(string(out), maxGrepBytes), Err: err}
	}
	return ToolResult{Output: truncateToolOutput(strings.TrimRight(string(out), "\n"), maxGrepBytes)}
}

func executeBash(ctx context.Context, workDir string, args map[string]any) ToolResult {
	command, ok := stringArg(args, "command")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: command")}
	}
	if err := validateShellCommand(command); err != nil {
		return ToolResult{Err: err}
	}

	shell := RunShellContext(ctx, workDir, command, nil)
	result := ToolResult{
		Output:    shell.Output,
		Cancelled: shell.Cancelled,
	}
	if shell.Cancelled {
		return result
	}
	if shell.Err != nil {
		result.Err = shell.Err
		return result
	}
	if shell.ExitCode != 0 {
		result.Output = formatShellExitOutput(shell.Output, shell.ExitCode)
	}
	return result
}

func validateShellCommand(command string) error {
	if command == "" {
		return errors.New("empty command")
	}
	if strings.Contains(command, "\x00") {
		return errors.New("command contains null byte")
	}
	_, err := syntax.NewParser().Parse(strings.NewReader(command), "")
	if err != nil {
		return fmt.Errorf("invalid shell syntax: %w", err)
	}
	return nil
}

func formatShellExitOutput(output string, exitCode int) string {
	if strings.TrimSpace(output) == "" {
		return fmt.Sprintf("(exit %d)", exitCode)
	}
	return output + fmt.Sprintf("\n\n(exit %d)", exitCode)
}

func executeGlob(workDir string, args map[string]any) ToolResult {
	pattern, ok := stringArg(args, "pattern")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: pattern")}
	}
	root := workDir
	if raw, ok := stringArg(args, "path"); ok && raw != "" {
		resolved, err := resolveWorkPath(workDir, raw)
		if err != nil {
			return ToolResult{Err: err}
		}
		root = resolved
	}
	if !strings.Contains(pattern, "/") {
		pattern = filepath.Join(root, pattern)
	}
	matches, err := filepath.Glob(pattern)
	if err != nil {
		return ToolResult{Err: err}
	}
	if len(matches) == 0 {
		return ToolResult{Output: "(no matches)"}
	}
	if len(matches) > maxGlobPaths {
		matches = matches[:maxGlobPaths]
	}
	out := strings.Join(matches, "\n")
	if len(matches) == maxGlobPaths {
		out += "\n\n(output truncated: path list capped)"
	}
	return ToolResult{Output: truncateToolOutput(out, maxGlobBytes)}
}

func truncateToolOutput(s string, maxBytes int) string {
	if maxBytes <= 0 || len(s) <= maxBytes {
		return s
	}
	notice := "\n\n(output truncated)"
	budget := maxBytes - len(notice)
	if budget <= 0 {
		return truncateUTF8(s, maxBytes)
	}
	return truncateUTF8(s, budget) + notice
}

func truncateUTF8(s string, maxBytes int) string {
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

func stringArg(args map[string]any, key string) (string, bool) {
	raw, ok := args[key]
	if !ok || raw == nil {
		return "", false
	}
	switch v := raw.(type) {
	case string:
		return strings.TrimSpace(v), v != ""
	default:
		return strings.TrimSpace(fmt.Sprint(v)), true
	}
}

func resolveWorkPath(workDir, path string) (string, error) {
	path = strings.TrimSpace(path)
	if path == "" {
		return "", errors.New("empty path")
	}
	if filepath.IsAbs(path) {
		return filepath.Clean(path), nil
	}
	if workDir == "" {
		return filepath.Clean(path), nil
	}
	return filepath.Clean(filepath.Join(workDir, path)), nil
}
