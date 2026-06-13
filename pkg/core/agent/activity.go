package agent

import (
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/renderer"
)

// Activity is the working-indicator label shown above the input prompt.
type Activity = constants.AgentActivity

// Activity constants re-exported for agent runtime code.
const (
	ActivityIdle       = constants.ActivityIdle
	ActivityConnecting = constants.ActivityConnecting
	ActivityLoading    = constants.ActivityLoading
	ActivityThinking   = constants.ActivityThinking
	ActivitySearching  = constants.ActivitySearching
	ActivityReading    = constants.ActivityReading
	ActivityWriting    = constants.ActivityWriting
	ActivityRunning    = constants.ActivityRunning
	ActivityFetching   = constants.ActivityFetching
	ActivityStreaming  = constants.ActivityStreaming
	ActivityPlanning   = constants.ActivityPlanning
	ActivityWaiting    = constants.ActivityWaiting
	ActivityWorking    = constants.ActivityWorking
)

// ActivityForTool maps a built-in tool name to an indicator label.
func ActivityForTool(tool string) Activity {
	return constants.ActivityForTool(tool)
}

// SetActivity returns a Bubble Tea command that updates the working indicator.
func SetActivity(activity Activity) tea.Cmd {
	return renderer.ActivityCmd(activity)
}

// SetActivityForTool returns a command that sets the indicator from a tool name.
func SetActivityForTool(tool string) tea.Cmd {
	return renderer.ActivityForToolCmd(tool)
}

// FinishTurn returns a command that ends the turn and appends the response.
func FinishTurn(response string) tea.Cmd {
	return renderer.AgentDoneCmd(response)
}
