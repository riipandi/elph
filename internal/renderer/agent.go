package renderer

import (
	"context"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/pkg/core/agent"
)

const spinnerInterval = 80 * time.Millisecond

var spinnerFrames = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

type spinnerTickMsg struct{}

func (m Model) showsActivity() bool {
	return m.agent.Busy || m.shell.Running
}

func (m Model) beginAgentTurn() Model {
	m.agent.Busy = true
	m.agent.Activity = agent.ActivityConnecting
	m.agent.SpinnerFrame = 0
	return m
}

func (m Model) beginShellActivity() Model {
	m.agent.Activity = agent.ActivityRunning
	m.agent.SpinnerFrame = 0
	return m
}

func (m Model) clearActivity() Model {
	if m.showsActivity() {
		return m
	}
	m.agent.Activity = agent.ActivityIdle
	m.agent.SpinnerFrame = 0
	return m
}

func (m Model) agentTurnCmds(prompt string) (Model, tea.Cmd) {
	ctx, cancel := context.WithCancel(context.Background())
	m.agent.Cancel = cancel
	events := m.session.StartTurn(ctx, prompt)
	m.agent.Events = events
	return m, tea.Batch(waitAgentEvent(events), m.spinnerTickCmd())
}

func (m Model) cancelAgentTurn() (Model, tea.Cmd) {
	m = m.cancelCtrlC()
	if m.agent.Cancel != nil {
		m.agent.Cancel()
		m.agent.Cancel = nil
	}
	m.agent.Events = nil
	m.agent.Busy = false
	m.agent.Activity = agent.ActivityIdle
	m.agent.SpinnerFrame = 0
	m, cmd := m.withMessage("(agent turn cancelled)")
	return m, cmd
}

func (m Model) spinnerTickCmd() tea.Cmd {
	if !m.showsActivity() {
		return nil
	}
	return tea.Tick(spinnerInterval, func(time.Time) tea.Msg { return spinnerTickMsg{} })
}

func (m Model) finishAgentTurn(response string) Model {
	m.agent.Cancel = nil
	m.agent.Events = nil
	m.agent.Busy = false
	m.agent.Activity = agent.ActivityIdle
	m.agent.SpinnerFrame = 0
	if strings.TrimSpace(response) != "" {
		m = m.addAIMessage(response)
	}
	m = m.syncLayout(true)
	return m
}