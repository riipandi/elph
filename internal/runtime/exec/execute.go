package exec

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/bmatcuk/doublestar/v4"
	"github.com/riipandi/elph/internal/runtime/shell"
	"github.com/riipandi/elph/internal/runtime/toolresult"
	"github.com/riipandi/elph/pkg/tools"
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

// bashToolTimeout is the runtime cap for agent Bash tool calls.
var bashToolTimeout = defaultBashTimeout

// ExecuteTool runs a built-in agent tool and returns its result.
func ExecuteTool(ctx context.Context, workDir, name string, args map[string]any) toolresult.ToolResult {
	return ExecuteToolWithOutput(ctx, workDir, name, args, nil)
}

// ExecuteToolWithOutput runs a built-in tool, streaming shell chunks to onChunk when supported.
func ExecuteToolWithOutput(ctx context.Context, workDir, name string, args map[string]any, onChunk func(string)) toolresult.ToolResult {
	canonical, known := tools.ResolveName(name)
	if !known {
		return toolresult.ToolResult{Err: toolresult.ErrToolUnknown}
	}
	if !tools.IsExecutable(canonical) {
		return toolresult.ToolResult{Err: toolresult.ErrToolUnavailable}
	}

	switch canonical {
	case tools.Read:
		return executeRead(workDir, args)
	case tools.Write:
		return executeWrite(workDir, args)
	case tools.Edit:
		return executeEdit(workDir, args)
	case tools.Grep:
		return executeGrep(ctx, workDir, args)
	case tools.Glob:
		return executeGlob(workDir, args)
	case tools.ReadMediaFile:
		return executeReadMediaFile(workDir, args)
	case tools.Bash:
		return executeBash(ctx, workDir, args, onChunk)
	case tools.WebSearch:
		return executeWebSearch(ctx, args)
	case tools.FetchURL:
		return executeFetchURL(ctx, args)
	case tools.CodeSearch:
		return executeCodeSearch(ctx, args)
	case tools.Skill:
		return executeSkill(ctx, workDir, args)
	case tools.TodoList:
		return executeTodoList(ctx, workDir, args)
	case tools.CreateGoal:
		return executeCreateGoal(ctx, args)
	case tools.GetGoal:
		return executeGetGoal(ctx, args)
	case tools.UpdateGoal:
		return executeUpdateGoal(ctx, args)
	case tools.SetGoalBudget:
		return executeSetGoalBudget(ctx, args)

	default:
		return toolresult.ToolResult{Err: fmt.Errorf("%w: %s", ErrToolNotImplemented, canonical)}
	}
}

func executeRead(workDir string, args map[string]any) toolresult.ToolResult {
	path, ok := stringArg(args, "path")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: path")}
	}
	full, err := resolveWorkPath(workDir, path)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	if dir, dirErr := checkIsDirectory(full); dirErr == nil && dir {
		return toolresult.ToolResult{
			Err: fmt.Errorf("%s is a directory — use Glob with pattern %q to list its contents", path, path+"/**"),
		}
	}

	lineOffset := intArg(args, "line_offset", 1)
	nLines := intArg(args, "n_lines", 0)

	data, err := os.ReadFile(full)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}

	lines := strings.Split(string(data), "\n")
	totalLines := len(lines)

	// Handle tail (negative offset)
	if lineOffset < 0 {
		lineOffset = totalLines + lineOffset + 1
		if lineOffset < 1 {
			lineOffset = 1
		}
	}

	// Cap lineOffset
	if lineOffset > totalLines {
		return toolresult.ToolResult{Output: "(end of file)"}
	}

	// Calculate line range
	start := lineOffset - 1 // zero-indexed
	end := totalLines
	if nLines > 0 && start+nLines < totalLines {
		truncated := start + nLines
		if truncated < totalLines {
			end = truncated
		}
	}

	// Build output with line numbers
	var out strings.Builder
	for i := start; i < end; i++ {
		fmt.Fprintf(&out, "%d\t%s\n", i+1, lines[i])
	}

	result := out.String()
	if len(result) > maxReadBytes {
		result = truncateUTF8(result, maxReadBytes)
		result += "\n\n(output truncated)"
	}

	// Add footer info
	totalRead := end - start
	footer := fmt.Sprintf("Total lines in file: %d. %d lines read.", totalLines, totalRead)
	if maxLinesReached := nLines > 0 && start+nLines < totalLines; maxLinesReached {
		footer += " (max lines reached)"
	} else if end < totalLines {
		footer += " (output truncated)"
	} else {
		footer += " End of file reached."
	}

	if len(result) > 0 {
		result += "\n"
	}
	result += "\n<system>" + footer + "</system>"

	return toolresult.ToolResult{Output: result}
}

func executeWrite(workDir string, args map[string]any) toolresult.ToolResult {
	path, ok := stringArg(args, "path")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: path")}
	}
	contents, ok := rawStringArg(args, "contents")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: contents")}
	}

	full, err := resolveWorkPath(workDir, path)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	if dir, dirErr := checkIsDirectory(full); dirErr == nil && dir {
		return toolresult.ToolResult{
			Err: fmt.Errorf("%s is a directory — cannot write to a directory path; use a file path or remove the directory first", path),
		}
	}

	if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
		return toolresult.ToolResult{Err: err}
	}

	mode := "overwrite"
	if raw, ok := stringArg(args, "mode"); ok && raw != "" {
		mode = strings.ToLower(raw)
	}

	switch mode {
	case "append":
		f, err := os.OpenFile(full, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
		if err != nil {
			return toolresult.ToolResult{Err: err}
		}
		defer f.Close()
		if _, err := f.WriteString(contents); err != nil {
			return toolresult.ToolResult{Err: err}
		}
		return toolresult.ToolResult{Output: fmt.Sprintf("Appended %d bytes to %s", len(contents), path)}
	default:
		if err := os.WriteFile(full, []byte(contents), 0o600); err != nil {
			return toolresult.ToolResult{Err: err}
		}
		return toolresult.ToolResult{Output: fmt.Sprintf("Wrote %d bytes to %s", len(contents), path)}
	}
}

func executeEdit(workDir string, args map[string]any) toolresult.ToolResult {
	path, ok := stringArg(args, "path")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: path")}
	}
	oldString, ok := rawStringArg(args, "old_string")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: old_string")}
	}
	newString, ok := rawStringArg(args, "new_string")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: new_string")}
	}

	// No-op guard
	if oldString == newString {
		return toolresult.ToolResult{Err: errors.New("no changes to make: old_string and new_string are exactly the same")}
	}

	full, err := resolveWorkPath(workDir, path)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	if dir, dirErr := checkIsDirectory(full); dirErr == nil && dir {
		return toolresult.ToolResult{
			Err: fmt.Errorf("%s is a directory — cannot edit a directory, use Glob with pattern %q to explore", path, path+"/**"),
		}
	}
	data, err := os.ReadFile(full)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	content := string(data)
	count := strings.Count(content, oldString)
	if count == 0 {
		return toolresult.ToolResult{Err: fmt.Errorf("old_string not found in %s — use Read to reload the file content", path)}
	}
	replaceAll := boolArg(args, "replace_all")
	if count > 1 && !replaceAll {
		return toolresult.ToolResult{Err: fmt.Errorf("old_string appears %d times in %s; set replace_all or provide more context", count, path)}
	}

	var updated string
	if replaceAll {
		updated = strings.ReplaceAll(content, oldString, newString)
	} else {
		updated = strings.Replace(content, oldString, newString, 1)
	}
	//nolint:gosec // deliberate file edit tool — path resolved via resolveWorkPath above
	if err := os.WriteFile(full, []byte(updated), 0o600); err != nil {
		return toolresult.ToolResult{Err: err}
	}
	replaced := count
	if !replaceAll {
		replaced = 1
	}
	return toolresult.ToolResult{Output: fmt.Sprintf("Replaced %d occurrence(s) in %s", replaced, path)}
}

func executeGrep(ctx context.Context, workDir string, args map[string]any) toolresult.ToolResult {
	pattern, ok := stringArg(args, "pattern")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: pattern")}
	}
	searchPath := workDir
	if raw, ok := stringArg(args, "path"); ok && raw != "" {
		resolved, err := resolveWorkPath(workDir, raw)
		if err != nil {
			return toolresult.ToolResult{Err: err}
		}
		searchPath = resolved
	}

	outputMode := "content"
	if raw, ok := stringArg(args, "output_mode"); ok && raw != "" {
		outputMode = strings.ToLower(raw)
	}

	cmdArgs := []string{"--regexp", pattern, "--color=never"}

	// Context lines (ripgrep -C)
	if raw := intArg(args, "context_lines", 0); raw > 0 {
		cmdArgs = append(cmdArgs, "-C", fmt.Sprint(raw))
	}

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
			return toolresult.ToolResult{Output: "(no matches)"}
		}
		return toolresult.ToolResult{Output: truncateToolOutput(string(out), maxGrepBytes), Err: err}
	}
	return toolresult.ToolResult{Output: truncateToolOutput(strings.TrimRight(string(out), "\n"), maxGrepBytes)}
}

func executeBash(ctx context.Context, workDir string, args map[string]any, onChunk func(string)) toolresult.ToolResult {
	command, ok := stringArg(args, "command")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: command")}
	}
	if err := validateShellCommand(command); err != nil {
		return toolresult.ToolResult{Err: err}
	}

	// Resolve working directory
	effectiveWd := workDir
	if raw, ok := stringArg(args, "cwd"); ok && raw != "" {
		resolved, err := resolveWorkPath(workDir, raw)
		if err != nil {
			return toolresult.ToolResult{Err: err}
		}
		effectiveWd = resolved
	}

	// Resolve timeout: default from package variable, override if provided
	bashTimeout := bashToolTimeout
	if raw := intArg(args, "timeout", 0); raw > 0 {
		bashTimeout = time.Duration(raw) * time.Second
		const maxTimeout = 300 * time.Second
		if bashTimeout > maxTimeout {
			bashTimeout = maxTimeout
		}
	}

	bashCtx, cancel := context.WithTimeout(ctx, bashTimeout)
	defer cancel()
	sh := shell.RunShellContext(bashCtx, effectiveWd, command, onChunk)
	result := toolresult.ToolResult{
		Output:    sh.Output,
		Cancelled: sh.Cancelled,
	}
	if sh.Cancelled {
		return result
	}
	if sh.Err != nil {
		result.Err = sh.Err
		return result
	}
	if sh.ExitCode != 0 {
		result.Output = formatShellExitOutput(sh.Output, sh.ExitCode)
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

func executeGlob(workDir string, args map[string]any) toolresult.ToolResult {
	pattern, ok := stringArg(args, "pattern")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: pattern")}
	}
	root := workDir
	if raw, ok := stringArg(args, "path"); ok && raw != "" {
		resolved, err := resolveWorkPath(workDir, raw)
		if err != nil {
			return toolresult.ToolResult{Err: err}
		}
		root = resolved
	}
	matches, err := globSearch(root, pattern)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	if len(matches) == 0 {
		return toolresult.ToolResult{Output: "(no matches)"}
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
	return toolresult.ToolResult{Output: truncateToolOutput(out, maxGlobBytes)}
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

func intArg(args map[string]any, key string, defaultVal int) int {
	raw, ok := args[key]
	if !ok || raw == nil {
		return defaultVal
	}
	switch v := raw.(type) {
	case int:
		return v
	case float64:
		return int(v)
	case string:
		n, err := strconv.Atoi(strings.TrimSpace(v))
		if err != nil {
			return defaultVal
		}
		return n
	default:
		return defaultVal
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

// checkIsDirectory reports whether the given path exists and is a directory.
func checkIsDirectory(path string) (bool, error) {
	info, err := os.Stat(path)
	if err != nil {
		if os.IsNotExist(err) {
			return false, nil
		}
		return false, err
	}
	return info.IsDir(), nil
}
