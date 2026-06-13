package agent

// EventKind identifies agent runtime events emitted during a turn.
type EventKind int

const (
	EventActivity EventKind = iota
	EventTurnDone
)

// Event is a framework-neutral agent runtime message.
type Event struct {
	Kind     EventKind
	Activity Activity
	Response string
}

// ActivityEvent returns an activity phase update.
func ActivityEvent(activity Activity) Event {
	return Event{Kind: EventActivity, Activity: activity}
}

// TurnDoneEvent returns a completed turn with the assistant response.
func TurnDoneEvent(response string) Event {
	return Event{Kind: EventTurnDone, Response: response}
}