package constants

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestActivityForTool(t *testing.T) {
	tests := []struct {
		tool string
		want AgentActivity
	}{
		{ToolRead, ActivityReading},
		{ToolReadMediaFile, ActivityReading},
		{ToolWrite, ActivityWriting},
		{ToolEdit, ActivityWriting},
		{ToolGrep, ActivitySearching},
		{ToolGlob, ActivitySearching},
		{ToolCodeSearch, ActivitySearching},
		{ToolWebSearch, ActivitySearching},
		{ToolBash, ActivityRunning},
		{ToolFetchURL, ActivityFetching},
		{ToolEnterPlanMode, ActivityPlanning},
		{ToolExitPlanMode, ActivityPlanning},
		{ToolAskUser, ActivityWaiting},
		{"UnknownTool", ActivityWorking},
		{"", ActivityWorking},
	}

	for _, tc := range tests {
		require.Equal(t, tc.want, ActivityForTool(tc.tool), "ActivityForTool(%q)", tc.tool)
	}
}

func TestAgentTurnPhasesOrder(t *testing.T) {
	want := []AgentActivity{
		ActivityConnecting,
		ActivityLoading,
		ActivityThinking,
		ActivitySearching,
		ActivityReading,
		ActivityWriting,
		ActivityRunning,
		ActivityStreaming,
	}
	require.Len(t, AgentTurnPhases, len(want))
	for i, phase := range want {
		require.Equal(t, phase, AgentTurnPhases[i], "phase[%d]", i)
	}
}
