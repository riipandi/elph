package constants

// AgentActivity describes what the agent is doing during a turn.
type AgentActivity string

const (
	ActivityIdle       AgentActivity = ""
	ActivityConnecting AgentActivity = "Connecting"
	ActivityLoading    AgentActivity = "Loading"
	ActivityThinking   AgentActivity = "Thinking"
	ActivitySearching  AgentActivity = "Searching"
	ActivityReading    AgentActivity = "Reading"
	ActivityWriting    AgentActivity = "Writing"
	ActivityRunning    AgentActivity = "Running"
	ActivityFetching   AgentActivity = "Fetching"
	ActivityStreaming  AgentActivity = "Streaming"
	ActivityPlanning   AgentActivity = "Planning"
	ActivityWaiting    AgentActivity = "Waiting"
	ActivityWorking    AgentActivity = "Working"
)

// AgentTurnPhases is the default ordered progression shown while a turn runs.
var AgentTurnPhases = []AgentActivity{
	ActivityConnecting,
	ActivityLoading,
	ActivityThinking,
	ActivitySearching,
	ActivityReading,
	ActivityWriting,
	ActivityRunning,
	ActivityStreaming,
}

// toolActivity maps built-in tool names to indicator labels.
var toolActivity = map[string]AgentActivity{
	ToolRead:          ActivityReading,
	ToolReadMediaFile: ActivityReading,
	ToolWrite:         ActivityWriting,
	ToolEdit:          ActivityWriting,
	ToolGrep:          ActivitySearching,
	ToolGlob:          ActivitySearching,
	ToolCodeSearch:    ActivitySearching,
	ToolWebSearch:     ActivitySearching,
	ToolBash:          ActivityRunning,
	ToolFetchURL:      ActivityFetching,
	ToolEnterPlanMode: ActivityPlanning,
	ToolExitPlanMode:  ActivityPlanning,
	ToolAskUser:       ActivityWaiting,
}

// ActivityForTool returns the indicator label for a tool call.
// Unknown tools fall back to ActivityWorking.
func ActivityForTool(tool string) AgentActivity {
	if activity, ok := toolActivity[tool]; ok {
		return activity
	}
	return ActivityWorking
}
