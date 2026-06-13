package agent

// ActivityMsg updates the working indicator label. Sent by the agent runtime
// when phase changes or a tool call starts.
type ActivityMsg struct {
	Activity Activity
}

// TurnDoneMsg signals a completed turn with the final assistant response.
type TurnDoneMsg struct {
	Response string
}