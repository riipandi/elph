package renderer

import (
	"context"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/settings"
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
	m.agent.ThinkingMsgID = -1
	m.agent.ResponseMsgID = -1
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

func (m Model) showThinkingEnabled() bool {
	cfg, err := settings.Load()
	if err != nil {
		return true
	}
	return cfg.ShowThinkingEnabled()
}

func (m Model) agentTurnCmds(prompt string) (Model, tea.Cmd) {
	ctx, cancel := context.WithCancel(context.Background())
	m.agent.Cancel = cancel
	events := m.session.StartTurn(ctx, prompt, m.showThinkingEnabled())
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
	if !m.showsActivity() && !m.modelsSyncingActive() {
		return nil
	}
	return tea.Tick(spinnerInterval, func(time.Time) tea.Msg { return spinnerTickMsg{} })
}

func (m Model) finishAgentTurn(thinking, response string) (Model, tea.Cmd) {
	m.agent.Cancel = nil
	m.agent.Events = nil
	m.agent.Busy = false
	m.agent.Activity = agent.ActivityIdle
	m.agent.SpinnerFrame = 0

	if m.showThinkingEnabled() && m.agent.ThinkingMsgID < 0 && strings.TrimSpace(thinking) != "" {
		m = m.addThinkingMessage(thinking)
		m.session.AppendLog("thinking", thinking)
	}

	responseIdx := m.agent.ResponseMsgID
	switch {
	case responseIdx >= 0 && strings.TrimSpace(response) != "":
		m.messages[responseIdx].text = response
		m.messages[responseIdx].renderCache = messageRenderCache{}
		m.messages[responseIdx].glamourPending = true
		m.session.AppendLog("ai", response)
		m.layout.ContentDirty = true
	case responseIdx < 0 && strings.TrimSpace(response) != "":
		m = m.addAIMessage(response)
		responseIdx = len(m.messages) - 1
	}

	m.agent.ThinkingMsgID = -1
	m.agent.ResponseMsgID = -1
	m.layout.StreamFlushPending = false
	m = m.clearStreamPrefixCache()
	m.layout.ContentDirty = true
	m = m.syncLayout(true)

	if responseIdx >= 0 {
		m, cmd := m.scheduleGlamourRender(responseIdx)
		m.layout.ContentDirty = false
		return m, cmd
	}
	return m, nil
}

func (m Model) appendAgentThinkingDelta(delta string) Model {
	if delta == "" || !m.showThinkingEnabled() {
		return m
	}
	if m.agent.ThinkingMsgID < 0 {
		m = m.addThinkingMessage(delta)
		m.agent.ThinkingMsgID = len(m.messages) - 1
	} else {
		idx := m.agent.ThinkingMsgID
		m.messages[idx].text += delta
		m.messages[idx].renderCache = messageRenderCache{}
		m.layout.ContentDirty = true
	}
	return m
}

func (m Model) appendAgentResponseDelta(delta string) Model {
	if delta == "" {
		return m
	}
	if m.agent.ResponseMsgID < 0 {
		m.messages = append(m.messages, message{text: delta, kind: constants.MessageAI})
		m.agent.ResponseMsgID = len(m.messages) - 1
		m.layout.ContentDirty = true
	} else {
		idx := m.agent.ResponseMsgID
		m.messages[idx].text += delta
		m.messages[idx].renderCache = messageRenderCache{}
		m.layout.ContentDirty = true
	}
	m.agent.Activity = agent.ActivityStreaming
	return m
}
