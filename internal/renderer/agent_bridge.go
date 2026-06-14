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
	m, cmd := m.applyAgentEvent(msg.event)
	if m.agent.Events == nil {
		return m, cmd
	}

	for {
		select {
		case evt, ok := <-m.agent.Events:
			if !ok {
				if cmd == nil {
					return m, func() tea.Msg { return agentTurnClosedMsg{} }
				}
				return m, tea.Batch(cmd, func() tea.Msg { return agentTurnClosedMsg{} })
			}
			switch evt.Kind {
			case agent.EventThinkingDelta, agent.EventResponseDelta:
				m, cmd = m.coalesceAgentEvent(cmd, evt)
				continue
			default:
				if cmd == nil {
					return m, waitAgentEvent(m.agent.Events)
				}
				return m, tea.Batch(cmd, func() tea.Msg { return agentEventMsg{event: evt} })
			}
		default:
			return m, cmd
		}
	}
}

func (m Model) coalesceAgentEvent(prior tea.Cmd, evt agent.Event) (Model, tea.Cmd) {
	next, nextCmd := m.applyAgentEvent(evt)
	if prior == nil {
		return next, nextCmd
	}
	if nextCmd == nil {
		return next, prior
	}
	return next, tea.Batch(prior, nextCmd)
}

func (m Model) applyAgentEvent(evt agent.Event) (Model, tea.Cmd) {
	switch evt.Kind {
	case agent.EventActivity:
		m.agent.Activity = evt.Activity
		if m.agent.Events != nil {
			return m, waitAgentEvent(m.agent.Events)
		}
		return m, nil
	case agent.EventThinkingDelta:
		if !m.thinkingTurnEnabled() {
			if m.agent.Events != nil {
				return m, waitAgentEvent(m.agent.Events)
			}
			return m, nil
		}
		m = m.appendAgentThinkingDelta(evt.Delta)
		return m.flushThinkingStreamNow()
	case agent.EventResponseDelta:
		m = m.appendAgentResponseDelta(evt.Delta)
		return m.markStreamDirty()
	case agent.EventToolCallStart:
		m.agent.Activity = agent.ActivityForTool(evt.ToolCall.Name)
		m = m.beginNativeToolCall(evt.ToolCall)
		m, cmd := m.markStreamDirty()
		if m.agent.ToolInteractBridge != nil {
			wait := waitToolInteractOffer(m.agent.ToolInteractBridge)
			if cmd == nil {
				return m, wait
			}
			return m, tea.Batch(cmd, wait)
		}
		return m, cmd
	case agent.EventToolCallDone:
		m = m.finishNativeToolCall(evt.ToolCall, evt.ToolResult)
		return m.markStreamDirty()
	case agent.EventTurnDone:
		m.turnCount++
		m = m.applyTurnUsage(evt.Usage)
		if len(evt.History) > 0 {
			m = m.applySessionHistory(evt.History)
		}
		return m.finishAgentTurn(evt.Thinking, evt.Response, evt.ProviderErr)
	}
	if m.agent.Events != nil {
		return m, waitAgentEvent(m.agent.Events)
	}
	return m, nil
}
