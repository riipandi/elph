package runtime

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/riipandi/elph/pkg/tool"
)

var ErrToolNotImplemented = errors.New("tool not implemented")

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
	const maxRead = 256 << 10
	if len(data) > maxRead {
		data = data[:maxRead]
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
		return ToolResult{Output: string(out), Err: err}
	}
	return ToolResult{Output: strings.TrimRight(string(out), "\n")}
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
	return ToolResult{Output: strings.Join(matches, "\n")}
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
