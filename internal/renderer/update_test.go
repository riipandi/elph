package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/prompt/template"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestCtrlCFirstPressWithInputShowsNotice(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, cmd := m.Update(keyCtrl('c'))
	m = updated.(Model)

	require.Equal(t, 1, m.ctrlCPress)
	require.NotNil(t, cmd)
	require.GreaterOrEqual(t, len(m.messages), 1)
}

func TestCtrlCSecondPressClearsInput(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, _ := m.Update(keyCtrl('c'))
	m = updated.(Model)
	require.Contains(t, m.messages[len(m.messages)-1].text, "Press again to exit")

	updated, cmd := m.Update(keyCtrl('c'))
	m = updated.(Model)

	require.Nil(t, cmd)
	require.Equal(t, 2, m.ctrlCPress)
	require.Empty(t, m.input.Value())
	require.Equal(t, -1, m.ctrlCNoticeID)
	for _, msg := range m.messages {
		require.NotContains(t, msg.text, "Press again to exit")
		require.NotContains(t, msg.text, "Input cleared")
	}
}

func TestCtrlCThirdPressQuits(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	for range 3 {
		updated, _ := m.Update(keyCtrl('c'))
		m = updated.(Model)
		if m.quitting {
			break
		}
	}

	require.True(t, m.quitting)
}

func TestCtrlCWithoutInputQuitsOnSecondPress(t *testing.T) {
	m := testInputModel(t)

	updated, _ := m.Update(keyCtrl('c'))
	m = updated.(Model)
	require.Equal(t, 1, m.ctrlCPress)

	updated, cmd := m.Update(keyCtrl('c'))
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestCtrlCResetClearsState(t *testing.T) {
	m := testInputModel(t)
	m.ctrlCPress = 1
	m.messages = []message{{text: "Press again to exit", kind: uiconst.MessageSystem}}
	m.ctrlCNoticeID = 0

	updated, _ := m.Update(ctrlCResetMsg{})
	m = updated.(Model)

	require.Equal(t, 0, m.ctrlCPress)
	require.Equal(t, -1, m.ctrlCNoticeID)
}

func TestOtherKeyCancelsCtrlCNotice(t *testing.T) {
	m := testInputModel(t)
	m.ctrlCPress = 1
	m.messages = []message{{text: "Press again to exit", kind: uiconst.MessageSystem}}
	m.ctrlCNoticeID = 0

	updated, _ := m.Update(keyRune('a'))
	m = updated.(Model)

	require.Equal(t, 0, m.ctrlCPress)
	require.Equal(t, -1, m.ctrlCNoticeID)
}

func TestCtrlDExitsImmediately(t *testing.T) {
	m := testInputModel(t)

	updated, cmd := m.Update(keyCtrl('d'))
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestCtrlASwitchesMode(t *testing.T) {
	m := testInputModel(t)
	prev := m.mode

	updated, _ := m.Update(keyCtrl('a'))
	m = updated.(Model)

	require.NotEqual(t, prev, m.mode)
	require.GreaterOrEqual(t, len(m.messages), 1)
}

func TestShiftTabCyclesThinking(t *testing.T) {
	m := testInputModel(t)
	prev := m.thinkingLevel

	updated, _ := m.Update(keyShiftTab())
	m = updated.(Model)

	require.NotEqual(t, prev, m.thinkingLevel)
}

func TestCtrlYCopiesLastMessage(t *testing.T) {
	m := testInputModel(t)
	m.messages = []message{{text: "last reply", kind: uiconst.MessageAI}}

	updated, _ := m.Update(keyCtrl('y'))
	m = updated.(Model)

	require.GreaterOrEqual(t, len(m.messages), 2)
}

func TestCtrlYNoMessagesNoOp(t *testing.T) {
	m := testInputModel(t)

	updated, _ := m.Update(keyCtrl('y'))
	m = updated.(Model)

	require.Empty(t, m.messages)
}

func TestNewlineWorksWhileBusy(t *testing.T) {
	m := testInputModel(t)
	m = m.beginAgentTurn()
	m.input.SetValue("draft")

	updated, cmd := m.Update(keyCtrlJ())
	m = updated.(Model)

	require.Nil(t, cmd)
	require.Equal(t, "draft\n", m.input.Value())
}

func TestSubmitColonQQuits(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue(":q")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestSubmitColonQBangQuits(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue(":q!")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestSubmitEmptyInputIgnored(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("   ")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.Nil(t, cmd)
	require.False(t, m.agent.Busy)
}

func TestSubmitSlashCommandHelp(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/help")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.Nil(t, cmd)
	require.False(t, m.agent.Busy)
	require.Len(t, m.messages, 2)
	require.Equal(t, uiconst.MessageUser, m.messages[0].kind)
	require.Equal(t, "/help", m.messages[0].text)
	require.Equal(t, uiconst.MessageSystem, m.messages[1].kind)
	require.Contains(t, m.messages[1].text, "/changelog")
	require.Contains(t, m.messages[1].text, "/diagnostic:list-tools")
}

func TestSubmitSlashCommandQuitExits(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/quit")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestSubmitSlashCommandExitExits(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/exit")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.True(t, m.quitting)
	require.NotNil(t, cmd)
}

func TestSuggestFuzzyQuitInPalette(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/qui")
	m = m.syncSlashSuggestions()

	require.True(t, m.commandPaletteActive())
	require.Equal(t, "exit", m.suggest.CmdSuggestions[0].Name)
}

func TestSubmitDiagnosticOpenLogWithoutArgExecutesSelectedArg(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log")
	m = m.syncSlashSuggestions()

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.Nil(t, cmd)
	require.False(t, m.agent.Busy)
	require.Equal(t, "/diagnostic:open-log system", m.messages[0].text)
	require.Equal(t, uiconst.MessageDetail, m.messages[1].kind)
	require.Equal(t, "Session log (system)", m.messages[1].detailLabel)
	require.True(t, m.messages[1].detailExpanded)
	require.Contains(t, m.messages[1].text, ".agents/elph/metadata/")
}

func TestSubmitDiagnosticOpenLogSystem(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log system")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.Nil(t, cmd)
	require.False(t, m.agent.Busy)
	require.Equal(t, uiconst.MessageDetail, m.messages[1].kind)
	require.Equal(t, "Session log (system)", m.messages[1].detailLabel)
	require.True(t, m.messages[1].detailExpanded)
	require.Contains(t, m.messages[1].text, ".agents/elph/metadata/")
}

func TestSubmitDiagnosticListTools(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:list-tools")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.Nil(t, cmd)
	require.False(t, m.agent.Busy)
	require.Equal(t, uiconst.MessageDetail, m.messages[1].kind)
	require.Equal(t, "Available tools", m.messages[1].detailLabel)
	require.True(t, m.messages[1].detailExpanded)
	require.Contains(t, m.messages[1].text, "Read")
	require.Contains(t, m.messages[1].text, "DiagnosticListTools")
}

func TestSubmitPromptTemplateStartsAgentTurn(t *testing.T) {
	m := testInputModel(t)
	m.promptTemplates = []template.Template{{
		Name:    "identify",
		Content: "Identify the codebase focusing on $1.",
	}}
	m.input.SetValue("/identify auth")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.NotNil(t, cmd)
	require.True(t, m.agent.Busy)
	require.Len(t, m.messages, 3)
	require.Equal(t, uiconst.MessageUser, m.messages[0].kind)
	require.Equal(t, "/identify auth", m.messages[0].text)
	require.Equal(t, uiconst.MessageDetail, m.messages[1].kind)
	require.Equal(t, "Prompt", m.messages[1].detailLabel)
	require.Contains(t, m.messages[1].text, "focusing on auth")
	require.Equal(t, uiconst.MessageThinking, m.messages[2].kind)
}

func TestSubmitSlashCommandUnknown(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/nope")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.Nil(t, cmd)
	require.False(t, m.agent.Busy)
	require.Contains(t, m.messages[1].text, "Unknown command")
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
	m.agent.SpinnerFrame = 0

	updated, cmd := m.Update(spinnerTickMsg{})
	m = updated.(Model)

	require.Equal(t, 1, m.agent.SpinnerFrame)
	require.NotNil(t, cmd)
}

func TestSpinnerTickWhenIdleNoOp(t *testing.T) {
	m := testInputModel(t)
	m.agent.SpinnerFrame = 0

	updated, cmd := m.Update(spinnerTickMsg{})
	m = updated.(Model)

	require.Equal(t, 0, m.agent.SpinnerFrame)
	require.Nil(t, cmd)
}

func TestSpinnerTickWhenShellRunning(t *testing.T) {
	m := testInputModel(t)
	m.shell.Running = true
	m.shell.Command = "whois example.com"
	m = m.beginShellActivity()
	m.agent.SpinnerFrame = 0

	updated, cmd := m.Update(spinnerTickMsg{})
	m = updated.(Model)

	require.Equal(t, 1, m.agent.SpinnerFrame)
	require.NotNil(t, cmd)
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
	require.Nil(t, cmd)
}

func TestReplaceNoticeUpdatesExisting(t *testing.T) {
	m := testInputModel(t)
	m.messages = []message{{text: "old notice", kind: uiconst.MessageSystem}}
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
	require.Equal(t, uiconst.MessageSystem, m.messages[0].kind)
}

func TestAddToolDetailAndThinkingMessages(t *testing.T) {
	m := New()
	m = m.addToolDetailMessage("Read", "file contents")
	m = m.addThinkingMessage("thinking...")
	require.Len(t, m.messages, 2)
	require.Equal(t, uiconst.MessageDetail, m.messages[0].kind)
	require.Equal(t, "Read", m.messages[0].detailLabel)
	require.False(t, m.messages[0].detailExpanded)
	require.Equal(t, uiconst.MessageThinking, m.messages[1].kind)
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
