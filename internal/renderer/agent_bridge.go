package renderer

import (
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/pkg/core/agent"
)

type agentEventMsg struct {
	event agent.Event
}

type agentTurnClosedMsg struct{}

func waitAgentEvent(ch <-chan agent.Event) tea.Cmd {
	return func() tea.Msg {
		evt, ok := <-ch
		if !ok {
			return agentTurnClosedMsg{}
		}
		return agentEventMsg{event: evt}
	}
}

func (m Model) handleAgentEvent(msg agentEventMsg) (Model, tea.Cmd) {
	switch msg.event.Kind {
	case agent.EventActivity:
		m.agent.Activity = msg.event.Activity
		m = m.syncLayout(m.content.AtBottom())
	case agent.EventThinkingDelta:
		if m.showThinkingEnabled() {
			m = m.appendAgentThinkingDelta(msg.event.Delta)
			return m.markStreamDirty()
		}
	case agent.EventResponseDelta:
		m = m.appendAgentResponseDelta(msg.event.Delta)
		return m.markStreamDirty()
	case agent.EventTurnDone:
		return m.finishAgentTurn(msg.event.Thinking, msg.event.Response)
	}
	if m.agent.Events != nil {
		return m, waitAgentEvent(m.agent.Events)
	}
	return m, nil
}
