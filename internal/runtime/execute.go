package runtime

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/bmatcuk/doublestar/v4"
	"github.com/riipandi/elph/pkg/tool"
	"mvdan.cc/sh/v3/syntax"
)

var ErrToolNotImplemented = errors.New("tool not implemented")

const (
	maxReadBytes = 256 << 10
	maxGrepBytes = 128 << 10
	maxGlobPaths = 500
	maxGlobBytes = 128 << 10

	// defaultBashTimeout caps agent Bash tool runtime so open-ended commands
	// (e.g. ping without -c) cannot block the turn indefinitely.
	defaultBashTimeout = 120 * time.Second
)

// bashToolTimeout is the runtime cap for agent Bash tool calls. Tests may lower it.
var bashToolTimeout = defaultBashTimeout

// ExecuteTool runs a built-in agent tool and returns its result.
func ExecuteTool(ctx context.Context, workDir, name string, args map[string]any) ToolResult {
	return ExecuteToolWithOutput(ctx, workDir, name, args, nil)
}

// ExecuteToolWithOutput runs a built-in tool, streaming shell chunks to onChunk when supported.
func ExecuteToolWithOutput(ctx context.Context, workDir, name string, args map[string]any, onChunk func(string)) ToolResult {
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
	case tool.Write:
		return executeWrite(workDir, args)
	case tool.Edit:
		return executeEdit(workDir, args)
	case tool.Grep:
		return executeGrep(ctx, workDir, args)
	case tool.Glob:
		return executeGlob(workDir, args)
	case tool.ReadMediaFile:
		return executeReadMediaFile(workDir, args)
	case tool.Bash:
		return executeBash(ctx, workDir, args, onChunk)
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

func executeWrite(workDir string, args map[string]any) ToolResult {
	path, ok := stringArg(args, "path")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: path")}
	}
	contents, ok := rawStringArg(args, "contents")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: contents")}
	}

	full, err := resolveWorkPath(workDir, path)
	if err != nil {
		return ToolResult{Err: err}
	}
	if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
		return ToolResult{Err: err}
	}
	if err := os.WriteFile(full, []byte(contents), 0o644); err != nil {
		return ToolResult{Err: err}
	}
	return ToolResult{Output: fmt.Sprintf("Wrote %d bytes to %s", len(contents), path)}
}

func executeEdit(workDir string, args map[string]any) ToolResult {
	path, ok := stringArg(args, "path")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: path")}
	}
	oldString, ok := rawStringArg(args, "old_string")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: old_string")}
	}
	newString, ok := rawStringArg(args, "new_string")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: new_string")}
	}

	full, err := resolveWorkPath(workDir, path)
	if err != nil {
		return ToolResult{Err: err}
	}
	data, err := os.ReadFile(full)
	if err != nil {
		return ToolResult{Err: err}
	}
	content := string(data)
	count := strings.Count(content, oldString)
	if count == 0 {
		return ToolResult{Err: fmt.Errorf("old_string not found in %s", path)}
	}
	replaceAll := boolArg(args, "replace_all")
	if count > 1 && !replaceAll {
		return ToolResult{Err: fmt.Errorf("old_string appears %d times in %s; set replace_all or provide more context", count, path)}
	}

	var updated string
	if replaceAll {
		updated = strings.ReplaceAll(content, oldString, newString)
	} else {
		updated = strings.Replace(content, oldString, newString, 1)
	}
	if err := os.WriteFile(full, []byte(updated), 0o644); err != nil {
		return ToolResult{Err: err}
	}
	replaced := count
	if !replaceAll {
		replaced = 1
	}
	return ToolResult{Output: fmt.Sprintf("Replaced %d occurrence(s) in %s", replaced, path)}
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

	outputMode := "content"
	if raw, ok := stringArg(args, "output_mode"); ok && raw != "" {
		outputMode = strings.ToLower(raw)
	}

	cmdArgs := []string{"--regexp", pattern, "--color=never"}
	switch outputMode {
	case "files_with_matches":
		cmdArgs = append(cmdArgs, "--files-with-matches")
	case "count":
		cmdArgs = append(cmdArgs, "--count")
	default:
		cmdArgs = append(cmdArgs, "--line-number", "--with-filename")
	}
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

func executeBash(ctx context.Context, workDir string, args map[string]any, onChunk func(string)) ToolResult {
	command, ok := stringArg(args, "command")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: command")}
	}
	if err := validateShellCommand(command); err != nil {
		return ToolResult{Err: err}
	}

	bashCtx, cancel := context.WithTimeout(ctx, bashToolTimeout)
	defer cancel()
	shell := RunShellContext(bashCtx, workDir, command, onChunk)
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
	matches, err := globSearch(root, pattern)
	if err != nil {
		return ToolResult{Err: err}
	}
	if len(matches) == 0 {
		return ToolResult{Output: "(no matches)"}
	}
	truncated := false
	if len(matches) > maxGlobPaths {
		matches = matches[:maxGlobPaths]
		truncated = true
	}
	out := strings.Join(matches, "\n")
	if truncated {
		out += "\n\n(output truncated: path list capped)"
	}
	return ToolResult{Output: truncateToolOutput(out, maxGlobBytes)}
}

func globSearch(root, pattern string) ([]string, error) {
	pattern = strings.TrimSpace(pattern)
	if pattern == "" {
		return nil, errors.New("empty pattern")
	}
	search := pattern
	if !filepath.IsAbs(pattern) {
		search = filepath.Join(root, pattern)
	}
	matches, err := doublestar.FilepathGlob(search, doublestar.WithFilesOnly())
	if err != nil {
		return nil, err
	}
	sort.Strings(matches)
	return matches, nil
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

func rawStringArg(args map[string]any, key string) (string, bool) {
	raw, ok := args[key]
	if !ok || raw == nil {
		return "", false
	}
	switch v := raw.(type) {
	case string:
		return v, true
	default:
		return fmt.Sprint(v), true
	}
}

func boolArg(args map[string]any, key string) bool {
	raw, ok := args[key]
	if !ok || raw == nil {
		return false
	}
	switch v := raw.(type) {
	case bool:
		return v
	case string:
		return strings.EqualFold(strings.TrimSpace(v), "true")
	default:
		return false
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
