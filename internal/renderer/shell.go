package renderer

import (
	"context"
	rtshell "github.com/riipandi/elph/internal/runtime/shell"
	"strings"
	"sync"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/uiconst"
)

type shellOutputMsg struct {
	chunk string
}

type shellOutputClosedMsg struct{}

type shellDoneMsg struct {
	result      rtshell.ShellResult
	command     string
	withContext bool
}

// parseShellCommand detects Pi-style shell prefixes.
// "!!cmd" runs without agent context; "!cmd" runs with context.
func parseShellCommand(s string) (command string, withContext bool, ok bool) {
	trimmed := strings.TrimLeft(s, " \t")
	if strings.HasPrefix(trimmed, "!!") {
		command = strings.TrimSpace(strings.TrimPrefix(trimmed, "!!"))
		return command, false, command != ""
	}
	if strings.HasPrefix(trimmed, "!") {
		command = strings.TrimSpace(strings.TrimPrefix(trimmed, "!"))
		return command, true, command != ""
	}
	return "", false, false
}

func isShellCancelKey(msg tea.KeyPressMsg) bool {
	if isInputEscapeKey(msg) {
		return true
	}
	return resolveKeyAction(msg) == uiconst.ActionQuit
}

func (m Model) addShellDetailMessage(label, body string, status uiconst.DetailStatus) Model {
	return m.addShellDetailMessageAt(label, body, status, time.Now())
}

func (m Model) addShellDetailMessageAt(label, body string, status uiconst.DetailStatus, at time.Time) Model {
	if at.IsZero() {
		at = time.Now()
	}
	m.messages = append(m.messages, message{
		text:           body,
		kind:           uiconst.MessageDetail,
		detailLabel:    label,
		detailStatus:   status,
		detailExpanded: true,
		at:             at,
	})
	m.layout.ContentDirty = true
	return m
}

func (m Model) handleShellSubmit(command string, withContext bool) (Model, tea.Cmd, bool) {
	if m.shell.Running {
		return m, nil, false
	}

	at := time.Now()
	m = m.addUserMessageAt(command, at)
	m = m.resetInput()

	m.shell.Running = true
	m.shell.Command = command
	m.shell.WithContext = withContext
	m.shell.Output = ""
	m = m.beginShellActivity()
	m = m.addShellDetailMessageAt(shellDetailLabel(command), "(running...)", uiconst.DetailStatusRunning, at)
	m.shell.DetailMsgID = len(m.messages) - 1
	m.layout.ContentDirty = true
	m = m.syncLayout(true)

	ctx, cancel := context.WithCancel(context.Background())
	m.shell.Cancel = cancel

	m.shell.OutputCh = make(chan string, 64)
	m.shell.DoneCh = make(chan rtshell.ShellResult, 1)

	outCh := m.shell.OutputCh
	doneCh := m.shell.DoneCh
	workDir := m.workDir

	start := func() tea.Msg {
		go func() {
			var sendMu sync.Mutex
			sendOpen := true
			sendChunk := func(chunk string) {
				defer func() { _ = recover() }()
				sendMu.Lock()
				defer sendMu.Unlock()
				if !sendOpen {
					return
				}
				select {
				case outCh <- chunk:
				case <-ctx.Done():
				}
			}

			result := rtshell.RunShellContext(ctx, workDir, command, sendChunk)

			sendMu.Lock()
			sendOpen = false
			sendMu.Unlock()
			closeOutCh(outCh)
			doneCh <- result
		}()
		return nil
	}

	return m, tea.Batch(
		func() tea.Msg { start(); return nil },
		waitShellOutput(outCh),
		waitShellDone(doneCh, command, withContext),
		m.spinnerTickCmd(),
		m.activityStopwatchStartCmd(),
	), true
}

func closeOutCh(ch chan string) {
	defer func() { _ = recover() }()
	close(ch)
}

func waitShellOutput(ch <-chan string) tea.Cmd {
	return func() tea.Msg {
		chunk, ok := <-ch
		if !ok {
			return shellOutputClosedMsg{}
		}
		return shellOutputMsg{chunk: chunk}
	}
}

func waitShellDone(ch <-chan rtshell.ShellResult, command string, withContext bool) tea.Cmd {
	return func() tea.Msg {
		result := <-ch
		return shellDoneMsg{
			result:      result,
			command:     command,
			withContext: withContext,
		}
	}
}

func (m Model) cancelShell() (Model, tea.Cmd) {
	if !m.shell.Running || m.shell.Cancel == nil {
		return m, nil
	}
	m = m.cancelCtrlC()
	m.shell.Cancel()
	return m, nil
}

func (m Model) updateShellDetailMessage(result *rtshell.ShellResult) Model {
	if m.shell.DetailMsgID < 0 || m.shell.DetailMsgID >= len(m.messages) {
		return m
	}

	var text string
	if result != nil {
		text = rtshell.FormatShellDetailBody(
			result.Output,
			result.ExitCode,
			result.Err,
			result.Cancelled,
		)
	} else if output := m.shell.Output; output == "" || isRunningDetailPlaceholder(output) {
		text = "(running...)"
	} else {
		text = output
	}

	idx := m.shell.DetailMsgID
	m.messages[idx].text = text
	if result != nil {
		m.messages[idx].detailStatus = shellDetailStatus(result, false)
	} else {
		m.messages[idx].detailStatus = uiconst.DetailStatusRunning
	}
	m.messages[idx].renderCache = messageRenderCache{}
	m.layout.ContentDirty = true
	return m
}

func (m Model) finishShellDone(msg shellDoneMsg) (Model, tea.Cmd) {
	m.shell.Cancel = nil
	m.shell.OutputCh = nil
	m.shell.DoneCh = nil

	m.shell.Output = msg.result.Output
	m = m.updateShellDetailMessage(&msg.result)

	if m.shell.DetailMsgID >= 0 && m.shell.DetailMsgID < len(m.messages) {
		m.session.AppendLog("shell", rtshell.FormatShellDisplay(
			m.shell.Command,
			msg.result.Output,
			msg.result.ExitCode,
			msg.result.Err,
			msg.result.Cancelled,
		))
	}
	m.shell.DetailMsgID = -1
	m.shell.Command = ""
	m.shell.Output = ""
	m.shell.Running = false

	if msg.withContext && !msg.result.Cancelled {
		prompt := rtshell.FormatShellContext(msg.command, msg.result.Output, msg.result.ExitCode)
		m.session.AppendLog("shell_context", prompt)
		if !m.hasActiveModel() {
			m, cmd := m.promptSelectModel()
			m = m.syncLayout(true)
			return m, cmd
		}
		m = m.beginAgentTurn()
		m = m.syncLayout(true)
		var agentCmd tea.Cmd
		m, agentCmd = m.agentTurnCmds(prompt, nil)
		return m, agentCmd
	}

	m = m.clearActivity()
	m = m.syncLayout(true)
	return m, refreshGitStatusCmd(m.workDir)
}

func (m Model) handleShellOutput(msg shellOutputMsg) (Model, tea.Cmd) {
	m.shell.Output = rtshell.ApplyStreamChunk(m.shell.Output, msg.chunk)
	m = m.updateShellDetailMessage(nil)

	// Don't call syncLayout directly on every chunk — that triggers a full
	// content rebuild (messagesView → renderMessageAt for every message)
	// on TUI output at Vt-100 baud rate. Instead, mark dirty and schedule a
	// throttled flush tick so the TUI render loop batches shell updates at
	// the same 40ms cadence as agent token streaming.
	var cmds []tea.Cmd
	if !m.layout.StreamFlushPending {
		m.layout.StreamFlushPending = true
		cmds = append(cmds, streamFlushTick())
	}

	var cmd tea.Cmd
	if m.shell.OutputCh != nil {
		cmd = waitShellOutput(m.shell.OutputCh)
	}
	if cmd != nil {
		cmds = append(cmds, cmd)
	}
	if len(cmds) == 0 {
		return m, nil
	}
	return m, tea.Batch(cmds...)
}
