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
		if m.agent.Events != nil {
			return m, waitAgentEvent(m.agent.Events)
		}
	case agent.EventTurnDone:
		m = m.finishAgentTurn(msg.event.Response)
	}
	return m, nil
}
