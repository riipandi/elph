package renderer

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestParseShellCommand(t *testing.T) {
	cases := []struct {
		input       string
		wantCmd     string
		wantContext bool
		wantOK      bool
	}{
		{"!ls", "ls", true, true},
		{"!!pwd", "pwd", false, true},
		{"  !! echo hi", "echo hi", false, true},
		{"!", "", true, false},
		{"!!", "", false, false},
		{"hello", "", false, false},
		{"/help", "", false, false},
	}
	for _, tc := range cases {
		cmd, withContext, ok := parseShellCommand(tc.input)
		require.Equal(t, tc.wantOK, ok, "input %q", tc.input)
		require.Equal(t, tc.wantCmd, cmd, "input %q", tc.input)
		require.Equal(t, tc.wantContext, withContext, "input %q", tc.input)
	}
}

func TestSubmitBareShellPrefixIgnored(t *testing.T) {
	for _, input := range []string{"!", "!!", "   !!   "} {
		m := testInputModel(t)
		m.input.SetValue(input)
		updated, cmd := m.Update(keyEnter())
		m = updated.(Model)
		require.Nil(t, cmd, "input %q", input)
		require.Empty(t, m.messages, "input %q", input)
	}
}

func dispatchTeaMsg(t *testing.T, m Model, msg tea.Msg) (Model, tea.Cmd) {
	t.Helper()
	if msg == nil {
		return m, nil
	}
	if batch, ok := msg.(tea.BatchMsg); ok {
		var followUps []tea.Cmd
		for _, sub := range batch {
			if sub == nil {
				continue
			}
			var next tea.Cmd
			m, next = dispatchTeaMsg(t, m, sub())
			if next != nil {
				followUps = append(followUps, next)
			}
		}
		return m, tea.Batch(followUps...)
	}
	updated, next := m.Update(msg)
	return updated.(Model), next
}

func runTeaCmd(t *testing.T, m Model, cmd tea.Cmd) Model {
	t.Helper()
	queue := []tea.Cmd{}
	if cmd != nil {
		queue = append(queue, cmd)
	}
	for len(queue) > 0 {
		current := queue[0]
		queue = queue[1:]
		if current == nil {
			continue
		}
		var next tea.Cmd
		m, next = dispatchTeaMsg(t, m, current())
		if next != nil {
			queue = append([]tea.Cmd{next}, queue...)
		}
	}
	return m
}

func waitForShellDone(t *testing.T, m Model, cmd tea.Cmd) Model {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		m = runTeaCmd(t, m, cmd)
		if !m.shell.Running {
			return m
		}
		cmd = nil
		time.Sleep(5 * time.Millisecond)
	}
	require.Fail(t, "timed out waiting for shell to finish")
	return m
}

func TestShellDetailExpandedByDefault(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("!!echo hello\nworld")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)
	require.True(t, m.messages[1].detailExpanded)

	m = waitForShellDone(t, m, cmd)
	require.True(t, m.messages[1].detailExpanded)

	rendered := stripANSI(m.renderMessageAt(1))
	require.Contains(t, rendered, "ctrl+o to collapse")
	require.Contains(t, rendered, "hello")
	require.Contains(t, rendered, "world")
}

func TestSubmitShellWithoutContext(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("!!echo shell-no-ctx")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)
	require.True(t, m.shell.Running)
	require.NotEmpty(t, m.activityView())
	require.Contains(t, stripANSI(m.activityView()), "Running Bash(echo shell-no-ctx)")
	require.Contains(t, stripANSI(m.activityView()), "Esc to cancel")

	m = waitForShellDone(t, m, cmd)

	require.False(t, m.agent.Busy)
	require.Len(t, m.messages, 2)
	require.Equal(t, uiconst.MessageUser, m.messages[0].kind)
	require.Equal(t, "echo shell-no-ctx", m.messages[0].text)
	require.Equal(t, uiconst.MessageDetail, m.messages[1].kind)
	require.Equal(t, "Bash(echo shell-no-ctx)", m.messages[1].detailLabel)
	require.Contains(t, m.messages[1].text, "shell-no-ctx")
	require.NotContains(t, m.messages[1].text, "$ echo shell-no-ctx")

	content := stripANSI(m.contentView())
	require.Contains(t, content, "shell-no-ctx")
}

func drainShellCmds(t *testing.T, m Model, cmd tea.Cmd) Model {
	t.Helper()
	batch, ok := cmd().(tea.BatchMsg)
	if !ok {
		return waitForShellDone(t, m, cmd)
	}

	var waitDone tea.Cmd
	for i, sub := range batch {
		if sub == nil || i > 2 {
			continue
		}
		if i == 2 {
			waitDone = sub
			continue
		}
		m, _ = dispatchTeaMsg(t, m, sub())
	}

	if waitDone == nil {
		return m
	}

	doneCh := make(chan tea.Msg, 1)
	go func() { doneCh <- waitDone() }()

	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		select {
		case msg := <-doneCh:
			if msg != nil {
				m, _ = dispatchTeaMsg(t, m, msg)
			}
			if !m.shell.Running {
				return m
			}
		default:
			if !m.shell.Running {
				return m
			}
			time.Sleep(5 * time.Millisecond)
		}
	}
	require.Fail(t, "timed out waiting for shell to finish")
	return m
}

func TestSubmitShellWithContext(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("!echo shell-with-ctx")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)
	require.True(t, m.shell.Running)

	m = drainShellCmds(t, m, cmd)
	if m.agent.Busy {
		if m.agent.Cancel != nil {
			m.agent.Cancel()
			m.agent.Cancel = nil
		}
		m.agent.Events = nil
		m.agent.ToolInteractBridge = nil
		m.agent.Busy = false
		m.agent.Activity = agent.ActivityIdle
		m.agent.SpinnerFrame = 0
		if thinkIdx := m.agent.ThinkingMsgID; thinkIdx >= 0 && thinkIdx < len(m.messages) {
			if strings.TrimSpace(m.messages[thinkIdx].text) == "" {
				m = m.removeMessageAt(thinkIdx)
			}
		}
		m.agent.ThinkingMsgID = -1
		m.agent.ResponseMsgID = -1
		m = m.clearStreamPrefixCache()
	}

	require.Len(t, m.messages, 2, "shell context should not add placeholder AI echo")
	require.False(t, m.agent.Busy)
	require.Equal(t, uiconst.MessageUser, m.messages[0].kind)
	require.Equal(t, "echo shell-with-ctx", m.messages[0].text)
	require.Equal(t, uiconst.MessageDetail, m.messages[1].kind)
	require.Equal(t, "Bash(echo shell-with-ctx)", m.messages[1].detailLabel)
	require.Contains(t, m.messages[1].text, "shell-with-ctx")
	require.Contains(t, stripANSI(m.contentView()), "shell-with-ctx")
	require.NotContains(t, stripANSI(m.contentView()), "Received:")
}

func splitShellBatchCmd(t *testing.T, cmd tea.Cmd) (tea.Cmd, tea.Cmd, tea.Cmd) {
	t.Helper()
	require.NotNil(t, cmd)
	batch, ok := cmd().(tea.BatchMsg)
	require.True(t, ok)
	require.GreaterOrEqual(t, len(batch), 3)
	return batch[0], batch[1], batch[2]
}

func TestCancelShellWithEscape(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue(`!!bash -c 'echo running; sleep 10'`)

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)
	require.True(t, m.shell.Running)

	startCmd, outCmd, doneCmd := splitShellBatchCmd(t, cmd)
	_ = startCmd()

	m, follow := dispatchTeaMsg(t, m, outCmd())
	require.NotNil(t, follow)

	updated, cancelCmd := m.Update(keyEscape())
	m = updated.(Model)
	require.Nil(t, cancelCmd)

	m, _ = dispatchTeaMsg(t, m, doneCmd())

	require.False(t, m.shell.Running)
	require.Equal(t, uiconst.MessageDetail, m.messages[1].kind)
	require.Contains(t, m.messages[1].text, "running")
	require.Contains(t, m.messages[1].text, "(cancelled)")
}

func TestCancelShellWithCtrlC(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue(`!!bash -c 'echo running; sleep 10'`)

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	startCmd, outCmd, doneCmd := splitShellBatchCmd(t, cmd)
	_ = startCmd()
	m, _ = dispatchTeaMsg(t, m, outCmd())

	updated, cancelCmd := m.Update(keyCtrl('c'))
	m = updated.(Model)
	require.Nil(t, cancelCmd)

	m, _ = dispatchTeaMsg(t, m, doneCmd())

	require.False(t, m.shell.Running)
	require.Contains(t, m.messages[1].text, "running")
	require.Contains(t, m.messages[1].text, "(cancelled)")
}

func TestShellWhileRunningBlocksSubmit(t *testing.T) {
	m := testInputModel(t)
	m.shell.Running = true
	m.input.SetValue("hello")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)
	require.Nil(t, cmd)
	require.Empty(t, m.messages)
}

func TestCancelShellPreservesPartialOutput(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue(`!!bash -c 'echo partial; sleep 10'`)

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	startCmd, outCmd, doneCmd := splitShellBatchCmd(t, cmd)
	_ = startCmd()
	m, _ = dispatchTeaMsg(t, m, outCmd())

	updated, _ = m.Update(keyEscape())
	m = updated.(Model)
	m, _ = dispatchTeaMsg(t, m, doneCmd())

	require.Contains(t, m.messages[1].text, "partial")
	require.Contains(t, m.messages[1].text, "(cancelled)")
	require.Contains(t, stripANSI(m.contentView()), "partial")
}

func keyEscape() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyEscape}
}
