package agent

import "github.com/riipandi/elph/pkg/tool"

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
	tool.Read:          ActivityReading,
	tool.ReadMediaFile: ActivityReading,
	tool.Write:         ActivityWriting,
	tool.Edit:          ActivityWriting,
	tool.Grep:          ActivitySearching,
	tool.Glob:          ActivitySearching,
	tool.CodeSearch:    ActivitySearching,
	tool.WebSearch:     ActivitySearching,
	tool.Bash:          ActivityRunning,
	tool.FetchURL:      ActivityFetching,
	tool.EnterPlanMode: ActivityPlanning,
	tool.ExitPlanMode:  ActivityPlanning,
	tool.AskUser:       ActivityWaiting,
	tool.Skill:         ActivityLoading,
}

// ActivityForTool returns the indicator label for a tool call.
// Unknown tools fall back to ActivityWorking.
func ActivityForTool(tool string) Activity {
	if activity, ok := toolActivity[tool]; ok {
		return activity
	}
	return ActivityWorking
}
