package catalog

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestBuiltinCatalogMatchesDocs(t *testing.T) {
	require.Len(t, All(), 15)

	cases := []struct {
		name                 string
		category             Category
		approval             Approval
		description          string
		requiresConfirmation bool
	}{
		{Read, CategoryFile, ApprovalAutoAllow, "Read a text file's contents", false},
		{Write, CategoryFile, ApprovalRequiresApproval, "Create or overwrite a file", false},
		{Edit, CategoryFile, ApprovalRequiresApproval, "Precise string replacement", false},
		{Grep, CategoryFile, ApprovalAutoAllow, "ripgrep powered full-text search", false},
		{Glob, CategoryFile, ApprovalAutoAllow, "Find files by glob pattern", false},
		{ReadMediaFile, CategoryFile, ApprovalAutoAllow, "Read an image or video file", false},
		{Bash, CategoryShell, ApprovalRequiresApproval, "Execute a shell command", false},
		{FetchURL, CategoryWeb, ApprovalAutoAllow, "Fetch the content of a specified URL", false},
		{WebSearch, CategoryWeb, ApprovalAutoAllow, "Web search with multiple engines", false},
		{CodeSearch, CategoryWeb, ApprovalAutoAllow, "Search code on GitHub (token optional) or GitLab", false},
		{EnterPlanMode, CategoryPlanMode, ApprovalAutoAllow, "Enter Plan mode", false},
		{ExitPlanMode, CategoryPlanMode, ApprovalAutoAllow, "Exit Plan mode and submit the plan", true},
		{TodoList, CategoryStateManagement, ApprovalAutoAllow, "Manage a task to-do list", false},
		{AskUser, CategoryCollaboration, ApprovalAutoAllow, "Ask the user a question to gather structured input", false},
		{Skill, CategoryCollaboration, ApprovalAutoAllow, "Invoke a registered inline Skill", false},
	}

	for _, tc := range cases {
		def, ok := Get(tc.name)
		require.True(t, ok, "Get(%q)", tc.name)
		require.Equal(t, tc.category, def.Category, "category for %q", tc.name)
		require.Equal(t, tc.approval, def.DefaultApproval, "approval for %q", tc.name)
		require.Equal(t, tc.description, def.Description, "description for %q", tc.name)
		require.Equal(t, tc.requiresConfirmation, def.RequiresConfirmation, "confirmation for %q", tc.name)
	}
}

func TestByCategory(t *testing.T) {
	require.Len(t, ByCategory(CategoryFile), 6)
	require.Len(t, ByCategory(CategoryShell), 1)
	require.Len(t, ByCategory(CategoryWeb), 3)
	require.Len(t, ByCategory(CategoryPlanMode), 2)
	require.Len(t, ByCategory(CategoryStateManagement), 1)
	require.Len(t, ByCategory(CategoryCollaboration), 2)
}

func TestRequiresApproval(t *testing.T) {
	require.False(t, RequiresApproval(Read))
	require.True(t, RequiresApproval(Write))
	require.True(t, RequiresApproval(Edit))
	require.True(t, RequiresApproval(Bash))
	require.False(t, RequiresApproval("UnknownTool"))
}

func TestApprovalConstants(t *testing.T) {
	require.Equal(t, Approval("auto-allow"), ApprovalAutoAllow)
	require.Equal(t, Approval("requires-approval"), ApprovalRequiresApproval)
	require.Equal(t, Approval("always-approve"), ApprovalAlwaysApprove)
}

func TestNamesPreservesCatalogOrder(t *testing.T) {
	require.Equal(t, []string{
		Read, Write, Edit, Grep, Glob, ReadMediaFile,
		Bash,
		FetchURL, WebSearch, CodeSearch,
		EnterPlanMode, ExitPlanMode,
		TodoList,
		AskUser, Skill,
	}, Names())
}
