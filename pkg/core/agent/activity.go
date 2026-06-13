package agent

// Activity describes what the agent is doing during a turn.
type Activity string

const (
	ActivityIdle       Activity = ""
	ActivityConnecting Activity = "Connecting"
	ActivityLoading    Activity = "Loading"
	ActivityThinking   Activity = "Thinking"
	ActivitySearching  Activity = "Searching"
	ActivityReading    Activity = "Reading"
	ActivityWriting    Activity = "Writing"
	ActivityRunning    Activity = "Running"
	ActivityFetching   Activity = "Fetching"
	ActivityStreaming  Activity = "Streaming"
	ActivityPlanning   Activity = "Planning"
	ActivityWaiting    Activity = "Waiting"
	ActivityWorking    Activity = "Working"
)

// TurnPhases is the default ordered progression shown while a turn runs.
var TurnPhases = []Activity{
	ActivityConnecting,
	ActivityLoading,
	ActivityThinking,
	ActivitySearching,
	ActivityReading,
	ActivityWriting,
	ActivityRunning,
	ActivityStreaming,
}

var toolActivity = map[string]Activity{
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
func ActivityForTool(tool string) Activity {
	if activity, ok := toolActivity[tool]; ok {
		return activity
	}
	return ActivityWorking
}