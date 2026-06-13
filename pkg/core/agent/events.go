package agent

// ActivityMsg and TurnDoneMsg are legacy Bubble Tea message types.
// TUI adapters in internal/renderer translate Event values into tea.Msg.
//
// Deprecated: use Event and the renderer agent bridge instead.
type ActivityMsg struct {
	Activity Activity
}

// TurnDoneMsg signals a completed turn with the final assistant response.
//
// Deprecated: use Event and the renderer agent bridge instead.
type TurnDoneMsg struct {
	Response string
}