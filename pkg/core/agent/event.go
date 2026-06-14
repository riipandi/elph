package agent

import "github.com/riipandi/elph/pkg/ai/provider"

// EventKind identifies agent runtime events emitted during a turn.
type EventKind int

const (
	EventActivity EventKind = iota
	EventThinkingDelta
	EventResponseDelta
	EventTurnDone
)

// Event is a framework-neutral agent runtime message.
type Event struct {
	Kind     EventKind
	Activity Activity
	Delta    string
	Thinking string
	Response string
}

// ActivityEvent returns an activity phase update.
func ActivityEvent(activity Activity) Event {
	return Event{Kind: EventActivity, Activity: activity}
}

// ThinkingDeltaEvent returns an incremental reasoning chunk.
func ThinkingDeltaEvent(delta string) Event {
	return Event{Kind: EventThinkingDelta, Delta: delta}
}

// ResponseDeltaEvent returns an incremental assistant response chunk.
func ResponseDeltaEvent(delta string) Event {
	return Event{Kind: EventResponseDelta, Delta: delta}
}

// TurnDoneEvent returns a completed turn with the final assistant response.
func TurnDoneEvent(result provider.TurnResult) Event {
	return Event{
		Kind:     EventTurnDone,
		Thinking: result.Thinking,
		Response: result.Content,
	}
}
