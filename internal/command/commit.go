package command

import (
	"fmt"
	"os/exec"
	"strings"
)

// defaultCommitTypes is the JSON type-to-description map matching Lumen's default.
const defaultCommitTypes = `{
  "docs": "Documentation only changes",
  "style": "Changes that do not affect the meaning of the code",
  "refactor": "A code change that neither fixes a bug nor adds a feature",
  "perf": "A code change that improves performance",
  "test": "Adding missing tests or correcting existing tests",
  "build": "Changes that affect the build system or external dependencies",
  "ci": "Changes to our CI configuration files and scripts",
  "chore": "Other changes that don't modify src or test files",
  "revert": "Reverts a previous commit",
  "feat": "A new feature",
  "fix": "A bug fix"
}`

func commitHandler(ctx *Context, args string) string {
	args = strings.TrimSpace(args)
	useUnstaged := strings.Contains(strings.ToLower(args), "--unstaged")

	// Everything except the --unstaged flag is context.
	// Preserve original case — don't lowercase the context text.
	var contextParts []string
	for _, p := range strings.Fields(args) {
		if strings.ToLower(p) == "--unstaged" {
			continue
		}
		contextParts = append(contextParts, p)
	}
	context := strings.Join(contextParts, " ")

	workDir := ctx.WorkDir

	// Determine diff source: staged (default) or working tree
	var diff string
	var diffSource string
	if useUnstaged {
		diff = runGitDiff(workDir, "diff", "--no-color")
		diffSource = "unstaged changes"
	} else {
		diff = runGitDiff(workDir, "diff", "--cached", "--no-color")
		diffSource = "staged changes"
	}

	if diff == "" {
		return fmt.Sprintf("/commit: no %s found. Stage your changes with 'git add' first.", diffSource)
	}

	// Replace the verbose project system prompt with a minimal commit-focused one
	// to save tokens and keep the model tightly scoped.
	ctx.ClearSystemPrompt = true
	ctx.SystemPromptOverride = `You are a commit message generator that follows these rules:
1. Write in present tense
2. Be concise and direct
3. Output only the commit message without any explanations
4. Follow the format: <type>(<optional scope>): <commit message>`

	// Build the context section if user provided additional intent.
	var contextSection string
	if context != "" {
		contextSection = fmt.Sprintf(`Use the following context to understand intent:
%s
`, context)
	}

	ctx.pendingAgentPrompt = fmt.Sprintf(`Generate a concise git commit message written in present tense for the following code diff with the given specifications below:

The output response must be in format:
<type>(<optional scope>): <commit message>
Choose a type from the type-to-description JSON below that best describes the git diff:
%s
Focus on being accurate and concise.
%sCommit message must be a maximum of 72 characters.
Exclude anything unnecessary such as translation. Your entire response will be passed directly into git commit.

Code diff (%s):
%s`, defaultCommitTypes, contextSection, diffSource, formatDiffForPrompt(diff))

	// Show the diff as a detail message
	ctx.pendingDetailLabel = "Commit diff"
	ctx.pendingDetailBody = fmt.Sprintf("Diff (%s):\n\n%s", diffSource, diff)
	ctx.pendingDetailExpanded = true

	return ""
}

func runGitDiff(workDir string, args ...string) string {
	cmd := exec.Command("git", args...)
	if workDir != "" {
		cmd.Dir = workDir
	}
	out, err := cmd.Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

func formatDiffForPrompt(diff string) string {
	const maxDiffLen = 8000
	if len(diff) > maxDiffLen {
		diff = diff[:maxDiffLen] + "\n\n(diff truncated)"
	}
	return "```diff\n" + diff + "\n```"
}
