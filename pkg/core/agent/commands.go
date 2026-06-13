package agent

// SetActivity returns an activity update event.
func SetActivity(activity Activity) Event {
	return ActivityEvent(activity)
}

// SetActivityForTool returns an activity update derived from a tool name.
func SetActivityForTool(tool string) Event {
	return ActivityEvent(ActivityForTool(tool))
}

// FinishTurn returns a turn-completion event.
func FinishTurn(response string) Event {
	return TurnDoneEvent(response)
}