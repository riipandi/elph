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

// primaryToolParamLabel returns a tool label that includes the most relevant
// parameter value, e.g. "Read(/path/to/file)" instead of just "Read".
// Long values are smart-truncated from the front using workDir context.
func primaryToolParamLabel(name string, params map[string]string, workDir string) string {
	oname, _ := tools.ResolveName(name)

	if oname == tools.Bash {
		cmd := params["command"]
		if cmd == "" {
			return ""
		}
		return "Bash(" + frontTruncate(cmd, 60, workDir) + ")"
	}

	key, ok := primaryToolParamKey(oname)
	if !ok {
		return ""
	}
	val := params[key]
	if val == "" {
		return ""
	}
	return oname + "(" + frontTruncate(val, 60, workDir) + ")"
}

// frontTruncate shortens a string from the front with dir-aware truncation.
// When workDir is non-empty, keeps suffix starting from the CWD folder name
// or nearest ancestor. Falls back to path-segment-aware char truncation.
func frontTruncate(s string, maxLen int, workDir string) string {
	if len(s) <= maxLen {
		return s
	}

	// Find the minimum cut point based on CWD overlap
	cut := len(s) - (maxLen - 3)
	if workDir != "" {
		cwdName := workDir[strings.LastIndexByte(workDir, '/')+1:]
		if idx := strings.LastIndex(s, "/"+cwdName); idx >= 0 {
			// Path goes through or under CWD — keep from CWD name
			cut = idx
		} else if common := commonPathPrefixLen(s, workDir); common > 0 {
			// Path shares common ancestor — keep from that level
			suffix := s[common:]
			if len(suffix)+3 <= maxLen {
				return "..." + suffix
			}
		}
	}

	// Ensure cut doesn't exceed path bounds
	if cut >= len(s) {
		cut = len(s) - (maxLen - 3)
	}
	if cut < 0 {
		cut = 0
	}

	// Round cut to nearest '/' for clean segment boundary
	if cut > 1 && strings.ContainsRune(s, '/') {
		if before := strings.LastIndexByte(s[:cut-1], '/'); before >= 0 {
			cut = before
		}
	}
	return "..." + s[cut:]
}

// commonPathPrefixLen returns the length of the longest common path prefix.
// e.g. "/a/b/c/d" and "/a/b/e/f" → 5 (the length of "/a/b")
func commonPathPrefixLen(a, b string) int {
	if a == "" || b == "" {
		return 0
	}
	i := 0
	for i < len(a) && i < len(b) && a[i] == b[i] {
		i++
	}
	if i < len(a) && i < len(b) {
		if lastSlash := strings.LastIndexByte(a[:i], '/'); lastSlash >= 0 {
			return lastSlash + 1
		}
	}
	return i
}

// primaryToolParamKey returns the parameter key that best identifies a tool call.
func primaryToolParamKey(name string) (string, bool) {
	switch name {
	case tools.Read, tools.Write, tools.Edit, tools.ReadMediaFile:
		return "path", true
	case tools.Grep, tools.Glob:
		return "pattern", true
	case tools.FetchURL:
		return "url", true
	case tools.WebSearch:
		return "query", true
	case tools.CodeSearch:
		return "query", true
	default:
		return "", false
	}
}

func (m Model) resetNativeToolState() Model {
	m.agent.NativeToolMsgIDs = nil
	m.agent.TodoListUpdating = false
	m.agent.TodoListBefore = nil
	return m
}

func nativeToolDetailLabel(call provider.ToolCall, workDir string) string {
	name, _ := tools.ResolveName(call.Name)
	args, err := agent.ParseToolArguments(call.Arguments)
	if err != nil {
		return name
	}
	if name == tools.Bash {
		if cmd, ok := bashCommandArg(args); ok {
			return shellDetailLabel(cmd)
		}
		return name
	}
	if label := primaryToolParamLabel(name, anyToStringMap(args), workDir); label != "" {
		return label
	}
	return name
}

// anyToStringMap converts map[string]any to map[string]string for use
// with primaryToolParamLabel. Non-string values and missing keys are skipped.
func anyToStringMap(args map[string]any) map[string]string {
	out := make(map[string]string, len(args))
	for k, v := range args {
		s, ok := v.(string)
		if ok {
			out[k] = s
		}
	}
	return out
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
	m = m.addToolDetailMessageWithStatus(nativeToolDetailLabel(call, m.workDir), "(running...)", uiconst.DetailStatusRunning)
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
			m.messages[idx].detailLabel = nativeToolDetailLabel(call, m.workDir)
		}
		label := m.messages[idx].detailLabel
		m.messages[idx].text = body
		m.messages[idx].detailStatus = status
		m.messages[idx].detailExpanded = toolDetailExpandedByDefault(label, body)
		m.messages[idx].renderCache = messageRenderCache{}
		m.layout.ContentDirty = true
		return m
	}

	return m.addToolDetailMessageWithStatus(nativeToolDetailLabel(call, m.workDir), body, status)
}

func (m Model) applySessionHistory(history []provider.ChatMessage) Model {
	m.session.ApplyHistory(history)
	return m
}
