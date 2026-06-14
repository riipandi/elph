package renderer

import (
	"fmt"
	"sort"
	"strings"

	"github.com/riipandi/elph/internal/runtime"
	"github.com/riipandi/elph/pkg/core/agent"
)

func (m Model) resetToolCallStreamFilter() Model {
	m.agent.ToolCallFilter.Reset()
	m.agent.ThinkTagFilter.Reset()
	m.agent.TurnToolCalls = nil
	m.agent.SeenToolCalls = nil
	m = m.resetNativeToolState()
	return m
}

func (m Model) toolCallSignature(call agent.ParsedToolCall) string {
	presentation := runtime.ResolveToolRequest(call.Name, call.Parameters)
	keys := make([]string, 0, len(call.Parameters))
	for key := range call.Parameters {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	var b strings.Builder
	b.WriteString(presentation.Name)
	for _, key := range keys {
		b.WriteByte('\x00')
		b.WriteString(key)
		b.WriteByte('=')
		b.WriteString(call.Parameters[key])
	}
	return b.String()
}

func (m Model) recordToolCallRequests(calls []agent.ParsedToolCall) Model {
	var recorded []agent.ParsedToolCall
	for _, call := range calls {
		sig := m.toolCallSignature(call)
		if m.agent.SeenToolCalls != nil {
			if _, ok := m.agent.SeenToolCalls[sig]; ok {
				continue
			}
		} else {
			m.agent.SeenToolCalls = make(map[string]struct{})
		}
		m.agent.SeenToolCalls[sig] = struct{}{}
		recorded = append(recorded, call)

		presentation := runtime.ResolveToolRequest(call.Name, call.Parameters)
		m = m.addToolDetailMessageWithStatus(
			presentation.Name,
			presentation.Body,
			toolRequestDetailStatus(presentation.Reason),
		)
		m.session.AppendLog("tool_request", fmt.Sprintf("%s %v", presentation.Name, call.Parameters))
	}
	if len(recorded) > 0 {
		m.agent.TurnToolCalls = append(m.agent.TurnToolCalls, recorded...)
		m = m.scrubToolPayloadsFromAIMessage()
	}
	return m
}

func (m Model) scrubToolPayloadsFromAIMessage() Model {
	idx := m.agent.ResponseMsgID
	if idx < 0 || idx >= len(m.messages) || len(m.agent.TurnToolCalls) == 0 {
		return m
	}
	m.messages[idx].text = agent.StripExtractedPayloads(m.messages[idx].text, m.agent.TurnToolCalls)
	m.messages[idx].renderCache = messageRenderCache{}
	if strings.TrimSpace(m.messages[idx].text) == "" {
		m = m.removeMessageAt(idx)
		m.agent.ResponseMsgID = -1
		return m
	}
	m.layout.ContentDirty = true
	return m
}

func (m Model) filterAgentResponseDelta(delta string) (string, []agent.ParsedToolCall) {
	return m.agent.ToolCallFilter.Process(delta)
}

func (m Model) finalizeAgentResponseText(response string) (string, []agent.ParsedToolCall) {
	return m.agent.ToolCallFilter.Flush(response)
}

func (m Model) removeMessageAt(index int) Model {
	if index < 0 || index >= len(m.messages) {
		return m
	}
	m.messages = append(m.messages[:index], m.messages[index+1:]...)
	m.layout.ContentDirty = true
	return m.clearStreamPrefixCache()
}
