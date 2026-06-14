package agent

import (
	"testing"

	"github.com/riipandi/elph/pkg/tool"
	"github.com/stretchr/testify/require"
)

func TestActivityForTool(t *testing.T) {
	tests := []struct {
		tool string
		want Activity
	}{
		{tool.Read, ActivityReading},
		{tool.ReadMediaFile, ActivityReading},
		{tool.Write, ActivityWriting},
		{tool.Edit, ActivityWriting},
		{tool.Grep, ActivitySearching},
		{tool.Glob, ActivitySearching},
		{tool.CodeSearch, ActivitySearching},
		{tool.WebSearch, ActivitySearching},
		{tool.Bash, ActivityRunning},
		{tool.FetchURL, ActivityFetching},
		{tool.EnterPlanMode, ActivityPlanning},
		{tool.ExitPlanMode, ActivityPlanning},
		{tool.AskUser, ActivityWaiting},
		{tool.Skill, ActivityLoading},
		{tool.TodoList, ActivityWorking},
		{"UnknownTool", ActivityWorking},
		{"", ActivityWorking},
	}

	for _, tc := range tests {
		require.Equal(t, tc.want, ActivityForTool(tc.tool), "ActivityForTool(%q)", tc.tool)
	}
}

func TestTurnPhasesOrder(t *testing.T) {
	want := []Activity{
		ActivityConnecting,
		ActivityLoading,
		ActivityThinking,
		ActivitySearching,
		ActivityReading,
		ActivityWriting,
		ActivityRunning,
		ActivityStreaming,
	}
	require.Len(t, TurnPhases, len(want))
	for i, phase := range want {
		require.Equal(t, phase, TurnPhases[i], "phase[%d]", i)
	}
}

func TestCommandsReturnEvents(t *testing.T) {
	actEvt := SetActivity(ActivityWriting)
	require.Equal(t, EventActivity, actEvt.Kind)
	require.Equal(t, ActivityWriting, actEvt.Activity)

	toolEvt := SetActivityForTool("read")
	require.Equal(t, EventActivity, toolEvt.Kind)
	require.NotEmpty(t, toolEvt.Activity)

	doneEvt := FinishTurn("response")
	require.Equal(t, EventTurnDone, doneEvt.Kind)
	require.Equal(t, "response", doneEvt.Response)
}

func TestPlaceholderResponse(t *testing.T) {
	got := PlaceholderResponse("hello")
	require.Contains(t, got, "hello")
	require.Contains(t, got, "placeholder")
}

func TestPlaceholderResponseShellContextEmpty(t *testing.T) {
	got := PlaceholderResponse("Ran `ls`\n```\nfile\n```")
	require.Empty(t, got)
}

func TestIsShellContextPrompt(t *testing.T) {
	require.True(t, IsShellContextPrompt("Ran `git status`\n```\n```"))
	require.False(t, IsShellContextPrompt("explain this code"))
}
