package renderer

import (
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/runtime"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/tool"
)

func (m Model) resetNativeToolState() Model {
	m.agent.NativeToolMsgIDs = nil
	return m
}

func (m Model) beginNativeToolCall(call provider.ToolCall) Model {
	name, _ := tool.ResolveName(call.Name)
	m = m.addToolDetailMessageWithStatus(name, "(running...)", constants.DetailStatusRunning)
	idx := len(m.messages) - 1
	if m.agent.NativeToolMsgIDs == nil {
		m.agent.NativeToolMsgIDs = make(map[string]int)
	}
	m.agent.NativeToolMsgIDs[call.ID] = idx
	return m
}

func (m Model) finishNativeToolCall(call provider.ToolCall, result agent.ToolRunResult) Model {
	runtimeResult := runtime.ToolResult{
		Output:    result.Output,
		Err:       result.Err,
		Cancelled: result.Cancelled,
	}
	name, _ := tool.ResolveName(call.Name)
	body := runtime.FormatToolDetailBodyFromResult(runtimeResult)
	status := toolDetailStatus(runtimeResult)

	if idx, ok := m.agent.NativeToolMsgIDs[call.ID]; ok && idx >= 0 && idx < len(m.messages) {
		m.messages[idx].detailLabel = name
		m.messages[idx].text = body
		m.messages[idx].detailStatus = status
		m.messages[idx].renderCache = messageRenderCache{}
		m.layout.ContentDirty = true
		return m
	}

	return m.addToolDetailMessageWithStatus(name, body, status)
}

func (m Model) applySessionHistory(history []provider.ChatMessage) Model {
	m.session.ApplyHistory(history)
	return m
}
