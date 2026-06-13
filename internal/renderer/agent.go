package renderer

import (
	"fmt"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/constants"
)

const (
	agentPhaseDelay = 400 * time.Millisecond
	spinnerInterval = 80 * time.Millisecond
)

var spinnerFrames = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

// ActivityMsg updates the working indicator label. Sent by the agent runtime
// when phase changes or a tool call starts.
type ActivityMsg struct {
	Activity constants.AgentActivity
}

// AgentDoneMsg signals a completed turn with the final assistant response.
type AgentDoneMsg struct {
	Response string
}

// ActivityCmd returns a command that sets the working indicator label.
func ActivityCmd(activity constants.AgentActivity) tea.Cmd {
	return func() tea.Msg { return ActivityMsg{Activity: activity} }
}

// ActivityForToolCmd sets the indicator from a built-in tool name.
func ActivityForToolCmd(tool string) tea.Cmd {
	return ActivityCmd(constants.ActivityForTool(tool))
}

// AgentDoneCmd returns a command that finishes the current agent turn.
func AgentDoneCmd(response string) tea.Cmd {
	return func() tea.Msg { return AgentDoneMsg{Response: response} }
}

type spinnerTickMsg struct{}

func (m Model) beginAgentTurn() Model {
	m.busy = true
	m.activity = constants.ActivityConnecting
	m.spinnerFrame = 0
	m.input.Blur()
	return m
}

func (m Model) agentTurnCmds(prompt string) tea.Cmd {
	cmds := []tea.Cmd{m.spinnerTickCmd()}

	for i, phase := range constants.AgentTurnPhases[1:] {
		delay := agentPhaseDelay * time.Duration(i+1)
		activity := phase
		cmds = append(cmds, tea.Tick(delay, func(time.Time) tea.Msg {
			return ActivityMsg{Activity: activity}
		}))
	}

	doneDelay := agentPhaseDelay * time.Duration(len(constants.AgentTurnPhases))
	cmds = append(cmds, tea.Tick(doneDelay, func(time.Time) tea.Msg {
		return AgentDoneMsg{Response: placeholderResponse(prompt)}
	}))

	return tea.Batch(cmds...)
}

func (m Model) spinnerTickCmd() tea.Cmd {
	if !m.busy {
		return nil
	}
	return tea.Tick(spinnerInterval, func(time.Time) tea.Msg { return spinnerTickMsg{} })
}

func (m Model) finishAgentTurn(response string) Model {
	m.busy = false
	m.activity = constants.ActivityIdle
	m.spinnerFrame = 0
	m.input.Focus()
	m = m.addAIMessage(response)
	m = m.syncLayout(true)
	return m
}

func placeholderResponse(prompt string) string {
	return fmt.Sprintf("Received: %s\n\n(Agent integration pending — this is a placeholder response.)", prompt)
}
