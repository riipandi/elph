package agent

import tea "charm.land/bubbletea/v2"

// SetActivity returns a Bubble Tea command that updates the working indicator.
func SetActivity(activity Activity) tea.Cmd {
	return func() tea.Msg { return ActivityMsg{Activity: activity} }
}

// SetActivityForTool returns a command that sets the indicator from a tool name.
func SetActivityForTool(tool string) tea.Cmd {
	return SetActivity(ActivityForTool(tool))
}

// FinishTurn returns a command that ends the turn with the given response.
func FinishTurn(response string) tea.Cmd {
	return func() tea.Msg { return TurnDoneMsg{Response: response} }
}