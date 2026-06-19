package renderer

import (
	"strings"

	"github.com/riipandi/elph/internal/runtime/shell"
	"github.com/riipandi/elph/internal/runtime/toolresult"

	"github.com/riipandi/elph/internal/uiconst"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/tools"
	"github.com/riipandi/elph/pkg/tools/todolist"
)

func (m Model) resetNativeToolState() Model {
	m.agent.NativeToolMsgIDs = nil
	m.agent.TodoListUpdating = false
	m.agent.TodoListBefore = nil
	return m
}

func nativeToolDetailLabel(call provider.ToolCall) string {
	name, _ := tools.ResolveName(call.Name)
	if name != tools.Bash {
		return name
	}
	args, err := agent.ParseToolArguments(call.Arguments)
	if err != nil {
		return name
	}
	if cmd, ok := bashCommandArg(args); ok {
		return shellDetailLabel(cmd)
	}
	return name
}

func bashCommandArg(args map[string]any) (string, bool) {
	raw, ok := args["command"]
	if !ok || raw == nil {
		return "", false
	}
	cmd, ok := raw.(string)
	if !ok {
		return "", false
	}
	cmd = strings.TrimSpace(cmd)
	return cmd, cmd != ""
}

func isTodoListTool(name string) bool {
	canonical, _ := tools.ResolveName(name)
	return canonical == tools.TodoList
}

func (m Model) beginNativeToolCall(call provider.ToolCall) Model {
	if canonical, ok := tools.ResolveName(call.Name); ok && canonical == tools.AskUser {
		if m.agent.NativeToolMsgIDs == nil {
			m.agent.NativeToolMsgIDs = make(map[string]int)
		}
		m.agent.NativeToolMsgIDs[call.ID] = -1
		return m
	}
	if isTodoListTool(call.Name) {
		m.agent.TodoListUpdating = true
		m.agent.TodoListBefore = append([]todolist.Todo(nil), m.session.Todos()...)
		if m.agent.NativeToolMsgIDs == nil {
			m.agent.NativeToolMsgIDs = make(map[string]int)
		}
		m.agent.NativeToolMsgIDs[call.ID] = -1
		return m
	}
	m = m.addToolDetailMessageWithStatus(nativeToolDetailLabel(call), "(running...)", uiconst.DetailStatusRunning)
	idx := len(m.messages) - 1
	if m.agent.NativeToolMsgIDs == nil {
		m.agent.NativeToolMsgIDs = make(map[string]int)
	}
	m.agent.NativeToolMsgIDs[call.ID] = idx
	return m
}

func (m Model) applyApprovalInteractUI(resp agent.ToolInteractResponse, req agent.ToolInteractRequest) Model {
	if req.Kind != agent.ToolInteractApproval || req.ToolCall.ID == "" {
		return m
	}
	switch {
	case resp.Cancelled:
		m = m.finishNativeToolCall(req.ToolCall, agent.ToolRunResult{Cancelled: true, Output: "User cancelled"})
	case !resp.Approved:
		m = m.finishNativeToolCall(req.ToolCall, agent.ToolRunResult{Output: agent.ToolDeniedMessage})
	default:
		return m
	}
	m.agent.Activity = agent.ActivityThinking
	m.layout.ContentDirty = true
	m = m.refreshStreamPrefixCache()
	return m
}

func (m Model) appendNativeToolOutput(call provider.ToolCall, delta string) Model {
	if isTodoListTool(call.Name) || delta == "" {
		return m
	}
	idx, ok := m.agent.NativeToolMsgIDs[call.ID]
	if !ok || idx < 0 || idx >= len(m.messages) {
		return m
	}
	if isRunningDetailPlaceholder(m.messages[idx].text) {
		m.messages[idx].text = ""
	}
	m.messages[idx].text = shell.ApplyStreamChunk(m.messages[idx].text, delta)
	m.messages[idx].renderCache = messageRenderCache{}
	m.layout.ContentDirty = true
	return m
}

func nativeToolDetailBody(name string, result toolresult.ToolResult, streamed string) string {
	var body string
	if name == tools.Bash {
		body = shell.FormatBashToolDetailBody(result, streamed)
	} else {
		body = toolresult.FormatToolDetailBodyFromResult(result)
	}
	return agent.TruncateWithNotice(body, agent.MaxDisplayToolBytes)
}

func nativeToolDetailStatus(name string, result toolresult.ToolResult) uiconst.DetailStatus {
	if name == tools.Bash {
		return bashToolDetailStatus(result)
	}
	return toolDetailStatus(result)
}

func (m Model) finishNativeToolCall(call provider.ToolCall, result agent.ToolRunResult) Model {
	if canonical, ok := tools.ResolveName(call.Name); ok && canonical == tools.AskUser {
		if m.agent.NativeToolMsgIDs != nil {
			delete(m.agent.NativeToolMsgIDs, call.ID)
		}
		return m
	}
	if isTodoListTool(call.Name) {
		m.agent.TodoListUpdating = false
		if m.agent.NativeToolMsgIDs != nil {
			delete(m.agent.NativeToolMsgIDs, call.ID)
		}
		after := m.session.Todos()
		before := m.agent.TodoListBefore
		m.agent.TodoListBefore = nil
		switch {
		case !todolist.AllDone(before) && todolist.AllDone(after):
			m = m.addTodoCompletionMessage(formatTodosCompletedMessage(after))
			m.session.ClearTodos()
		case todolist.AllDone(after):
			m.session.ClearTodos()
		}
		m = m.syncLayout(m.content.AtBottom())
		return m
	}
	runtimeResult := toolresult.ToolResult{
		Output:    result.Output,
		Err:       result.Err,
		Cancelled: result.Cancelled,
	}
	name, _ := tools.ResolveName(call.Name)

	var streamed string
	if idx, ok := m.agent.NativeToolMsgIDs[call.ID]; ok && idx >= 0 && idx < len(m.messages) {
		if !isRunningDetailPlaceholder(m.messages[idx].text) {
			streamed = m.messages[idx].text
		}
	}

	body := nativeToolDetailBody(name, runtimeResult, streamed)
	status := nativeToolDetailStatus(name, runtimeResult)

	if idx, ok := m.agent.NativeToolMsgIDs[call.ID]; ok && idx >= 0 && idx < len(m.messages) {
		if strings.TrimSpace(m.messages[idx].detailLabel) == "" {
			m.messages[idx].detailLabel = nativeToolDetailLabel(call)
		}
		label := m.messages[idx].detailLabel
		m.messages[idx].text = body
		m.messages[idx].detailStatus = status
		m.messages[idx].detailExpanded = toolDetailExpandedByDefault(label, body)
		m.messages[idx].renderCache = messageRenderCache{}
		m.layout.ContentDirty = true
		return m
	}

	return m.addToolDetailMessageWithStatus(nativeToolDetailLabel(call), body, status)
}

func (m Model) applySessionHistory(history []provider.ChatMessage) Model {
	m.session.ApplyHistory(history)
	return m
}
