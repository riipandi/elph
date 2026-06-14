package renderer

import (
	"context"
	"strings"
	"sync"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/git"
	"github.com/riipandi/elph/internal/runtime"
)

type shellOutputMsg struct {
	chunk string
}

type shellOutputClosedMsg struct{}

type shellDoneMsg struct {
	result      runtime.ShellResult
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
	return resolveKeyAction(msg) == constants.ActionQuit
}

func (m Model) addShellDetailMessage(label, body string, status constants.DetailStatus) Model {
	return m.addShellDetailMessageAt(label, body, status, time.Now())
}

func (m Model) addShellDetailMessageAt(label, body string, status constants.DetailStatus, at time.Time) Model {
	if at.IsZero() {
		at = time.Now()
	}
	m.messages = append(m.messages, message{
		text:           body,
		kind:           constants.MessageDetail,
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
	m = m.addShellDetailMessageAt(shellDetailLabel(command), "(running...)", constants.DetailStatusRunning, at)
	m.shell.DetailMsgID = len(m.messages) - 1
	m.layout.ContentDirty = true
	m = m.syncLayout(true)

	ctx, cancel := context.WithCancel(context.Background())
	m.shell.Cancel = cancel

	m.shell.OutputCh = make(chan string, 64)
	m.shell.DoneCh = make(chan runtime.ShellResult, 1)

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
				case outCh <- runtime.SanitizeStreamChunk(chunk):
				case <-ctx.Done():
				}
			}

			result := runtime.RunShellContext(ctx, workDir, command, sendChunk)

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

func waitShellDone(ch <-chan runtime.ShellResult, command string, withContext bool) tea.Cmd {
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

func (m Model) updateShellDetailMessage(result *runtime.ShellResult) Model {
	if m.shell.DetailMsgID < 0 || m.shell.DetailMsgID >= len(m.messages) {
		return m
	}

	var text string
	if result != nil {
		text = runtime.FormatShellDetailBody(
			result.Output,
			result.ExitCode,
			result.Err,
			result.Cancelled,
		)
	} else {
		output := runtime.TrimStreamOutput(m.shell.Output)
		if output == "" {
			text = "(running...)"
		} else {
			text = output
		}
	}

	idx := m.shell.DetailMsgID
	m.messages[idx].text = text
	if result != nil {
		m.messages[idx].detailStatus = shellDetailStatus(result, false)
	} else {
		m.messages[idx].detailStatus = constants.DetailStatusRunning
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
		m.session.AppendLog("shell", runtime.FormatShellDisplay(
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
		prompt := runtime.FormatShellContext(msg.command, msg.result.Output, msg.result.ExitCode)
		m.session.AppendLog("shell_context", prompt)
		m = m.beginAgentTurn()
		m = m.syncLayout(true)
		var agentCmd tea.Cmd
		m, agentCmd = m.agentTurnCmds(prompt)
		return m, agentCmd
	}

	m = m.clearActivity()
	m = m.applyGitStatus(git.Read(m.workDir))
	m = m.syncLayout(true)
	return m, refreshGitStatusCmd(m.workDir)
}

func (m Model) handleShellOutput(msg shellOutputMsg) (Model, tea.Cmd) {
	m.shell.Output += msg.chunk
	m = m.updateShellDetailMessage(nil)
	m = m.syncLayout(true)

	var cmd tea.Cmd
	if m.shell.OutputCh != nil {
		cmd = waitShellOutput(m.shell.OutputCh)
	}
	return m, cmd
}
