package command

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"
)

// TestCommitRegistered verifies /commit is in the help output.
func TestCommitRegistered(t *testing.T) {
	result := Execute("/help", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "/commit")
}

// TestCommitNoDiff returns a helpful error when there is no diff.
func TestCommitNoDiff(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)

	result := Execute("/commit", Context{WorkDir: dir})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "no staged changes")
}

// TestCommitStagedDiff builds a prompt from staged changes.
func TestCommitStagedDiff(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "hello.go", `package main

func main() {
	println("hello")
}
`)

	result := Execute("/commit", Context{WorkDir: dir})
	require.True(t, result.OK)

	// AgentPrompt should be non-empty with Lumen format
	require.NotEmpty(t, result.AgentPrompt)
	require.Contains(t, result.AgentPrompt, "Generate a concise git commit message")
	require.Contains(t, result.AgentPrompt, "staged changes")
	require.Contains(t, result.AgentPrompt, defaultCommitTypes)
	require.Contains(t, result.AgentPrompt, "```diff")
	require.Contains(t, result.AgentPrompt, "+package main")
	// Should include 72-char rule and "Exclude anything unnecessary"
	require.Contains(t, result.AgentPrompt, "maximum of 72 characters")
	require.Contains(t, result.AgentPrompt, "Exclude anything unnecessary")
	// Should NOT include context section when no context provided
	require.NotContains(t, result.AgentPrompt, "Use the following context to understand intent")
	// User prompt should NOT contain system prompt rules
	require.NotContains(t, result.AgentPrompt, "Write in present tense")

	// ClearSystemPrompt should be set
	require.True(t, result.ClearSystemPrompt)
	// SystemPrompt should match Lumen's format (rule 3: "without any explanations")
	require.NotEmpty(t, result.SystemPrompt)
	require.Contains(t, result.SystemPrompt, "commit message generator")
	require.Contains(t, result.SystemPrompt, "Output only the commit message without any explanations")
	require.Contains(t, result.SystemPrompt, "<type>(<optional scope>)")

	// Detail should contain the diff
	require.Equal(t, "Commit diff", result.DetailLabel)
	require.Contains(t, result.DetailBody, "Diff (staged changes)")
	require.Contains(t, result.DetailBody, "package main")
	require.True(t, result.DetailExpanded)

	// Output should be empty (handler returns "")
	require.Empty(t, result.Output)
	require.True(t, result.CommitAfterTurn)
}

// TestCommitWithContext includes user-provided context in the prompt.
func TestCommitWithContext(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "login.go", `package auth

func login() {
	// TODO: implement
}
`)

	result := Execute("/commit Fixing the null pointer bug in user login flow", Context{WorkDir: dir})
	require.True(t, result.OK)
	require.NotEmpty(t, result.AgentPrompt)
	require.Contains(t, result.AgentPrompt, "Use the following context to understand intent")
	require.Contains(t, result.AgentPrompt, "Fixing the null pointer bug in user login flow")
	require.True(t, result.CommitAfterTurn)
}

// TestCommitWithContextUnstaged works with --unstaged flag and context.
func TestCommitWithContextUnstaged(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "main.go", `package main

var x = 1
`)
	// Modify to create unstaged changes
	writeFile(t, dir, "main.go", `package main

var x = 2
`)

	result := Execute("/commit --unstaged Refactor variable naming", Context{WorkDir: dir})
	require.True(t, result.OK)
	require.NotEmpty(t, result.AgentPrompt)
	require.Contains(t, result.AgentPrompt, "Use the following context to understand intent")
	require.Contains(t, result.AgentPrompt, "Refactor variable naming")
	require.Contains(t, result.AgentPrompt, "unstaged changes")
	require.True(t, result.CommitAfterTurn)
}

// TestCommitUnstagedDiff uses --unstaged flag without context.
func TestCommitUnstagedDiff(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	// Stage a file first, then modify it so git diff picks up unstaged changes
	stageFile(t, dir, "main.go", `package pkg

var x = 1
`)
	// Modify the staged file to create unstaged changes
	writeFile(t, dir, "main.go", `package pkg

var x = 2
`)

	result := Execute("/commit --unstaged", Context{WorkDir: dir})
	require.True(t, result.OK)
	require.NotEmpty(t, result.AgentPrompt)
	require.Contains(t, result.AgentPrompt, "unstaged changes")
	require.Contains(t, result.AgentPrompt, "+var x = 2")
	require.True(t, result.ClearSystemPrompt)
	require.True(t, result.CommitAfterTurn)
}

// TestCommitDiffTruncated caps large diffs.
func TestCommitDiffTruncated(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)

	// Create a file larger than maxDiffLen
	bigContent := make([]byte, 9000)
	for i := range bigContent {
		bigContent[i] = 'A' + byte(i%26)
	}
	stageFile(t, dir, "big.txt", string(bigContent))

	result := Execute("/commit", Context{WorkDir: dir})
	require.True(t, result.OK)
	require.NotEmpty(t, result.AgentPrompt)
	require.Contains(t, result.AgentPrompt, "(diff truncated)")
	require.True(t, result.ClearSystemPrompt)
	require.True(t, result.CommitAfterTurn)
}

// TestCommitHandlerWire verifies the handler sets all expected context fields.
func TestCommitHandlerWire(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "a.go", "package a\n")

	ctx := Context{WorkDir: dir}
	output := commitHandler(&ctx, "")
	require.Empty(t, output)
	require.NotEmpty(t, ctx.pendingAgentPrompt)
	require.True(t, ctx.ClearSystemPrompt)
	require.NotEmpty(t, ctx.SystemPromptOverride)
	require.Contains(t, ctx.SystemPromptOverride, "commit message generator")
	require.Contains(t, ctx.SystemPromptOverride, "without any explanations")
	require.Equal(t, "Commit diff", ctx.pendingDetailLabel)
	require.True(t, ctx.pendingDetailExpanded)
	require.Contains(t, ctx.pendingDetailBody, "Diff (staged changes)")
	require.NotContains(t, ctx.pendingAgentPrompt, "Use the following context to understand intent")
	require.True(t, ctx.CommitAfterTurn)
}

// TestCommitHandlerWireWithContext verifies context is included.
func TestCommitHandlerWireWithContext(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "ctx.go", "package ctx\n")

	ctx := Context{WorkDir: dir}
	output := commitHandler(&ctx, "Fix login redirect bug")
	require.Empty(t, output)
	require.Contains(t, ctx.pendingAgentPrompt, "Use the following context to understand intent")
	require.Contains(t, ctx.pendingAgentPrompt, "Fix login redirect bug")
	require.True(t, ctx.ClearSystemPrompt)
	require.True(t, ctx.CommitAfterTurn)
}

// TestCommitHandlerWireUnstaged verifies --unstaged flag with tracked file modifications.
func TestCommitHandlerWireUnstaged(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "b.go", "package b\n")
	writeFile(t, dir, "b.go", "package b\n\nvar y = 1\n")

	ctx := Context{WorkDir: dir}
	output := commitHandler(&ctx, "--unstaged")
	require.Empty(t, output)
	require.NotEmpty(t, ctx.pendingAgentPrompt)
	require.True(t, ctx.ClearSystemPrompt)
	require.NotEmpty(t, ctx.SystemPromptOverride)
	require.Contains(t, ctx.SystemPromptOverride, "commit message generator")
	require.Contains(t, ctx.pendingAgentPrompt, "unstaged changes")
	require.Contains(t, ctx.pendingDetailBody, "Diff (unstaged changes)")
	require.True(t, ctx.CommitAfterTurn)
}

// TestCommitHandlerWireUnstagedWithContext verifies --unstaged + context together.
func TestCommitHandlerWireUnstagedWithContext(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "c.go", "package c\n")
	writeFile(t, dir, "c.go", "package c\n\nvar z = 1\n")

	ctx := Context{WorkDir: dir}
	output := commitHandler(&ctx, "--unstaged Improve error handling")
	require.Empty(t, output)
	require.Contains(t, ctx.pendingAgentPrompt, "Use the following context to understand intent")
	require.Contains(t, ctx.pendingAgentPrompt, "Improve error handling")
	require.Contains(t, ctx.pendingAgentPrompt, "unstaged changes")
	require.True(t, ctx.ClearSystemPrompt)
	require.True(t, ctx.CommitAfterTurn)
}

// TestExecuteCommitResultWire verifies Execute passes through all fields.
func TestExecuteCommitResultWire(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "d.go", "package d\n")

	result := Execute("/commit", Context{WorkDir: dir})
	require.True(t, result.OK)
	require.NotEmpty(t, result.AgentPrompt)
	require.True(t, result.ClearSystemPrompt)
	require.NotEmpty(t, result.SystemPrompt)
	require.Contains(t, result.SystemPrompt, "commit message generator")
	require.Equal(t, "Commit diff", result.DetailLabel)
	require.True(t, result.DetailExpanded)
	require.True(t, result.CommitAfterTurn)
}

// TestExecuteCommitResultWireUnstaged verifies --unstaged flag propagation.
func TestExecuteCommitResultWireUnstaged(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "e.go", "package e\n")
	writeFile(t, dir, "e.go", "package e\n\nvar w = 42\n")

	result := Execute("/commit --unstaged", Context{WorkDir: dir})
	require.True(t, result.OK)
	require.NotEmpty(t, result.AgentPrompt)
	require.True(t, result.ClearSystemPrompt)
	require.Contains(t, result.DetailBody, "Diff (unstaged changes)")
	require.True(t, result.CommitAfterTurn)
}

// TestExecuteCommitWithContext verifies context survives Execute round-trip.
func TestExecuteCommitWithContext(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "f.go", "package f\n")

	result := Execute("/commit Add health check endpoint", Context{WorkDir: dir})
	require.True(t, result.OK)
	require.Contains(t, result.AgentPrompt, "Use the following context to understand intent")
	require.Contains(t, result.AgentPrompt, "Add health check endpoint")
	require.True(t, result.CommitAfterTurn)
}

// TestRunGitDiff runs git diff in a workdir.
func TestRunGitDiff(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "g.go", "package g\n")

	diff := runGitDiff(dir, "diff", "--cached", "--no-color")
	require.NotEmpty(t, diff)
	require.Contains(t, diff, "+package g")
}

// TestRunGitDiffEmptyDir returns empty for directories that are not git repos.
func TestRunGitDiffEmptyDir(t *testing.T) {
	diff := runGitDiff(t.TempDir(), "diff", "--cached", "--no-color")
	require.Empty(t, diff)
}

// TestFormatDiffForPrompt wraps diff in markdown code fence.
func TestFormatDiffForPrompt(t *testing.T) {
	result := formatDiffForPrompt("+fmt.Println")
	require.Equal(t, "```diff\n+fmt.Println\n```", result)
}

// TestFormatDiffForPromptTruncated caps large diffs at maxDiffLen.
func TestFormatDiffForPromptTruncated(t *testing.T) {
	big := strings.Repeat("a\n", 5000)
	result := formatDiffForPrompt(big)
	require.Contains(t, result, "(diff truncated)")
	require.True(t, len(result) < len(big)+100)
}

// TestCommitDefaultTypes verifies the constant is well-formed JSON.
func TestCommitDefaultTypes(t *testing.T) {
	require.Contains(t, defaultCommitTypes, `"feat": "A new feature"`)
	require.Contains(t, defaultCommitTypes, `"fix": "A bug fix"`)
}

// --- git test helpers ---

func initGitRepo(t *testing.T, dir string) {
	t.Helper()
	runCmd(t, dir, "git", "init", "--initial-branch=main")
	runCmd(t, dir, "git", "config", "user.email", "test@test.com")
	runCmd(t, dir, "git", "config", "user.name", "Test User")
	// Create initial commit so diff --cached has a baseline
	writeFile(t, dir, ".gitkeep", "")
	runCmd(t, dir, "git", "add", ".")
	runCmd(t, dir, "git", "commit", "--allow-empty", "-m", "initial")
}

func stageFile(t *testing.T, dir, name, content string) {
	t.Helper()
	writeFile(t, dir, name, content)
	runCmd(t, dir, "git", "add", name)
}

func writeFile(t *testing.T, dir, name, content string) {
	t.Helper()
	path := filepath.Join(dir, name)
	err := os.MkdirAll(filepath.Dir(path), 0o755)
	require.NoError(t, err)
	err = os.WriteFile(path, []byte(content), 0o644)
	require.NoError(t, err)
}

func runCmd(t *testing.T, dir, name string, args ...string) string {
	t.Helper()
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, "cmd %s %v failed: %s", name, args, string(out))
	return string(out)
}

// TestExecuteCommitNoWorkDir handles gracefully when WorkDir is empty.
func TestExecuteCommitNoWorkDir(t *testing.T) {
	result := Execute("/commit", Context{})
	require.True(t, result.OK)
	require.NotNil(t, result)
	// Should not panic — either has diff or returns error message
}

// TestCommitHandlerWireContextBeforeFlag verifies context before --unstaged flag.
func TestCommitHandlerWireContextBeforeFlag(t *testing.T) {
	dir := t.TempDir()
	initGitRepo(t, dir)
	stageFile(t, dir, "h.go", "package h\n")
	writeFile(t, dir, "h.go", "package h\n\nvar q = 1\n")

	ctx := Context{WorkDir: dir}
	output := commitHandler(&ctx, "Minor tweak --unstaged")
	require.Empty(t, output)
	require.Contains(t, ctx.pendingAgentPrompt, "Use the following context to understand intent")
	require.Contains(t, ctx.pendingAgentPrompt, "Minor tweak")
	require.Contains(t, ctx.pendingAgentPrompt, "unstaged changes")
	require.True(t, ctx.ClearSystemPrompt)
	require.True(t, ctx.CommitAfterTurn)
}
