package renderer

import (
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/pkg/core/agent"
)

const spinnerInterval = 80 * time.Millisecond

var spinnerFrames = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

type spinnerTickMsg struct{}

func (m Model) showsActivity() bool {
	return m.busy || m.shellRunning
}

func (m Model) beginAgentTurn() Model {
	m.busy = true
	m.activity = agent.ActivityConnecting
	m.spinnerFrame = 0
	return m
}

func (m Model) beginShellActivity() Model {
	m.activity = agent.ActivityRunning
	m.spinnerFrame = 0
	return m
}

func (m Model) clearActivity() Model {
	if m.showsActivity() {
		return m
	}
	m.activity = agent.ActivityIdle
	m.spinnerFrame = 0
	return m
}

func (m Model) agentTurnCmds(prompt string) tea.Cmd {
	return tea.Batch(m.session.RunTurn(prompt), m.spinnerTickCmd())
}

func (m Model) spinnerTickCmd() tea.Cmd {
	if !m.showsActivity() {
		return nil
	}
	return tea.Tick(spinnerInterval, func(time.Time) tea.Msg { return spinnerTickMsg{} })
}

func (m Model) finishAgentTurn(response string) Model {
	m.busy = false
	m.activity = agent.ActivityIdle
	m.spinnerFrame = 0
	if strings.TrimSpace(response) != "" {
		m = m.addAIMessage(response)
	}
	m = m.syncLayout(true)
	return m
}
