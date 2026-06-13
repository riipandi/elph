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

func TestCommandsReturnMessages(t *testing.T) {
	actCmd := SetActivity(ActivityWriting)
	require.Equal(t, ActivityWriting, actCmd().(ActivityMsg).Activity)

	toolCmd := SetActivityForTool("read")
	require.NotEmpty(t, toolCmd().(ActivityMsg).Activity)

	doneCmd := FinishTurn("response")
	require.Equal(t, "response", doneCmd().(TurnDoneMsg).Response)
}

func TestPlaceholderResponse(t *testing.T) {
	got := PlaceholderResponse("hello")
	require.Contains(t, got, "hello")
	require.Contains(t, got, "placeholder")
}

func TestRunTurnReturnsCommand(t *testing.T) {
	require.NotNil(t, RunTurn("test prompt"))
}