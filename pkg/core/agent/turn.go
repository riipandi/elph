package agent

import (
	"fmt"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
)

const PhaseDelay = 400 * time.Millisecond

// IsShellContextPrompt reports Pi-style shell output queued for the agent (!cmd).
func IsShellContextPrompt(prompt string) bool {
	return strings.HasPrefix(strings.TrimSpace(prompt), "Ran `")
}

// RunTurn returns commands that simulate an agent turn until a response is ready.
// Real provider and tool integration will replace the placeholder completion.
func RunTurn(prompt string) tea.Cmd {
	if IsShellContextPrompt(prompt) {
		return func() tea.Msg {
			return TurnDoneMsg{Response: PlaceholderResponse(prompt)}
		}
	}

	cmds := make([]tea.Cmd, 0, len(TurnPhases))

	for i, phase := range TurnPhases[1:] {
		delay := PhaseDelay * time.Duration(i+1)
		activity := phase
		cmds = append(cmds, tea.Tick(delay, func(time.Time) tea.Msg {
			return ActivityMsg{Activity: activity}
		}))
	}

	doneDelay := PhaseDelay * time.Duration(len(TurnPhases))
	cmds = append(cmds, tea.Tick(doneDelay, func(time.Time) tea.Msg {
		return TurnDoneMsg{Response: PlaceholderResponse(prompt)}
	}))

	return tea.Batch(cmds...)
}

// PlaceholderResponse is a stub assistant reply used until provider integration lands.
func PlaceholderResponse(prompt string) string {
	if IsShellContextPrompt(prompt) {
		// Shell output is already shown in the chat; context is logged for the agent.
		return ""
	}
	return fmt.Sprintf("Received: %s\n\n(Agent integration pending — this is a placeholder response.)", prompt)
}
