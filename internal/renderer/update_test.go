package renderer

import (
	"testing"

	"github.com/charmbracelet/bubbletea"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestCtrlCFirstPressWithInputShowsNotice(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	m = updated.(Model)

	require.Equal(t, 1, m.ctrlCPress)
	require.NotNil(t, cmd)
	require.GreaterOrEqual(t, len(m.messages), 1)
}

func TestCtrlCSecondPressClearsInput(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	m = updated.(Model)
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	m = updated.(Model)

	require.Equal(t, 2, m.ctrlCPress)
	require.Empty(t, m.input.Value())
}

func TestCtrlCThirdPressQuits(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	for range 3 {
		updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
		m = updated.(Model)
		if m.quitting {
			break
		}
	}

	require.True(t, m.quitting)
}

func TestCtrlCWithoutInputQuitsOnSecondPress(t *testing.T) {
	m := testInputModel(t)

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	m = updated.(Model)
	require.Equal(t, 1, m.ctrlCPress)

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestCtrlCResetClearsState(t *testing.T) {
	m := testInputModel(t)
	m.ctrlCPress = 1
	m.messages = []message{{text: "Press again to exit", kind: constants.MessageSystem}}
	m.ctrlCNoticeID = 0

	updated, _ := m.Update(ctrlCResetMsg{})
	m = updated.(Model)

	require.Equal(t, 0, m.ctrlCPress)
	require.Equal(t, -1, m.ctrlCNoticeID)
}

func TestOtherKeyCancelsCtrlCNotice(t *testing.T) {
	m := testInputModel(t)
	m.ctrlCPress = 1
	m.messages = []message{{text: "Press again to exit", kind: constants.MessageSystem}}
	m.ctrlCNoticeID = 0

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'a'}})
	m = updated.(Model)

	require.Equal(t, 0, m.ctrlCPress)
	require.Equal(t, -1, m.ctrlCNoticeID)
}

func TestCtrlDExitsImmediately(t *testing.T) {
	m := testInputModel(t)

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyCtrlD})
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestCtrlASwitchesMode(t *testing.T) {
	m := testInputModel(t)
	prev := m.mode

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlA})
	m = updated.(Model)

	require.NotEqual(t, prev, m.mode)
	require.GreaterOrEqual(t, len(m.messages), 1)
}

func TestShiftTabCyclesThinking(t *testing.T) {
	m := testInputModel(t)
	prev := m.thinkingLevel

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyShiftTab})
	m = updated.(Model)

	require.NotEqual(t, prev, m.thinkingLevel)
}

func TestCtrlYCopiesLastMessage(t *testing.T) {
	m := testInputModel(t)
	m.messages = []message{{text: "last reply", kind: constants.MessageAI}}

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlY})
	m = updated.(Model)

	require.GreaterOrEqual(t, len(m.messages), 2)
}

func TestCtrlYNoMessagesNoOp(t *testing.T) {
	m := testInputModel(t)

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlY})
	m = updated.(Model)

	require.Empty(t, m.messages)
}

func TestBusyBlocksNewline(t *testing.T) {
	m := testInputModel(t)
	m = m.beginAgentTurn()

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyCtrlJ})
	m = updated.(Model)

	require.Nil(t, cmd)
	require.Empty(t, m.input.Value())
}

func TestSubmitColonQQuits(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue(":q")

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestSubmitColonQBangQuits(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue(":q!")

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestSubmitEmptyInputIgnored(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("   ")

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(Model)

	require.Nil(t, cmd)
	require.False(t, m.busy)
}

func TestSubmitStripsTriggerPrefixes(t *testing.T) {
	cases := []struct {
		input string
		want  string
	}{
		{"!ls", "ls"},
		{"!!rm -rf", "rm -rf"},
		{"/help", "help"},
	}
	for _, tc := range cases {
		m := testInputModel(t)
		m.input.SetValue(tc.input)
		updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
		m = updated.(Model)
		require.NotNil(t, cmd, "input %q", tc.input)
		require.Len(t, m.messages, 1, "input %q", tc.input)
		require.Equal(t, tc.want, m.messages[0].text, "input %q", tc.input)
	}
}

func TestSyncPromptPrefixChangesChar(t *testing.T) {
	cases := []struct {
		input string
		want  string
	}{
		{"", ">"},
		{"!run", "$"},
		{"!!sudo", "#"},
		{"/mode", "/"},
		{"  !x", "$"},
	}
	for _, tc := range cases {
		m := testInputModel(t)
		m.input.SetValue(tc.input)
		m = m.syncPromptPrefix()
		require.Equal(t, tc.want, m.promptChar, "input %q", tc.input)
	}
}

func TestSpinnerTickWhenBusy(t *testing.T) {
	m := testInputModel(t)
	m = m.beginAgentTurn()
	m.spinnerFrame = 0

	updated, cmd := m.Update(spinnerTickMsg{})
	m = updated.(Model)

	require.Equal(t, 1, m.spinnerFrame)
	require.NotNil(t, cmd)
}

func TestSpinnerTickWhenIdleNoOp(t *testing.T) {
	m := testInputModel(t)
	m.spinnerFrame = 0

	updated, cmd := m.Update(spinnerTickMsg{})
	m = updated.(Model)

	require.Equal(t, 0, m.spinnerFrame)
	require.Nil(t, cmd)
}

func TestTermFeaturesMsgHandled(t *testing.T) {
	m := testInputModel(t)
	updated, cmd := m.Update(termFeaturesMsg{})
	m = updated.(Model)
	require.NotNil(t, updated)
	require.Nil(t, cmd)
	_ = m
}

func TestMouseReenableMsgResumesCapture(t *testing.T) {
	m := testInputModel(t)
	m.mouseEnabled = false
	m.selectingText = true

	updated, cmd := m.Update(mouseReenableMsg{})
	m = updated.(Model)

	require.True(t, m.mouseEnabled)
	require.False(t, m.selectingText)
	require.NotNil(t, cmd)
}

func TestReplaceNoticeUpdatesExisting(t *testing.T) {
	m := testInputModel(t)
	m.messages = []message{{text: "old notice", kind: constants.MessageSystem}}
	m.ctrlCNoticeID = 0

	updated, _ := m.replaceNotice("new notice")
	m = updated

	require.Equal(t, "new notice", m.messages[0].text)
}

func TestReplaceNoticeAppendsWhenMissing(t *testing.T) {
	m := testInputModel(t)
	m.ctrlCNoticeID = -1

	updated, _ := m.replaceNotice("fresh notice")
	m = updated

	require.Len(t, m.messages, 1)
	require.Equal(t, "fresh notice", m.messages[0].text)
	require.Equal(t, 0, m.ctrlCNoticeID)
}

func TestWithMessageAppendsSystemMessage(t *testing.T) {
	m := testInputModel(t)
	updated, cmd := m.withMessage("system info")
	m = updated
	require.Nil(t, cmd)
	require.Len(t, m.messages, 1)
	require.Equal(t, constants.MessageSystem, m.messages[0].kind)
}

func TestAddToolAndThinkingMessages(t *testing.T) {
	m := New()
	m = m.addToolMessage("tool output")
	m = m.addThinkingMessage("thinking...")
	require.Len(t, m.messages, 2)
	require.Equal(t, constants.MessageTool, m.messages[0].kind)
	require.Equal(t, constants.MessageThinking, m.messages[1].kind)
}

func TestShowPromptPrefixRendersInInput(t *testing.T) {
	m := testInputModel(t)
	m.showPromptPrefix = true
	m.promptChar = "$"
	m.input.SetValue("ls")
	m = m.syncInputWidth()

	view := m.inputView()
	require.Contains(t, stripANSI(view), "$")
}