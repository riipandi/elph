package renderer

import (
	"io"
	"os"
	"strings"
	"testing"

	"charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestNextAndPrevMode(t *testing.T) {
	require.Equal(t, constants.ModePlan, nextMode(constants.ModeBuild))
	require.Equal(t, constants.ModeAsk, nextMode(constants.ModePlan))
	require.Equal(t, constants.ModeBrave, nextMode(constants.ModeAsk))
	require.Equal(t, constants.ModeBuild, nextMode(constants.ModeBrave))
	require.Equal(t, constants.ModeBuild, nextMode("unknown"))

	require.Equal(t, constants.ModeBrave, prevMode(constants.ModeBuild))
	require.Equal(t, constants.ModeBuild, prevMode(constants.ModePlan))
	require.Equal(t, constants.ModePlan, prevMode(constants.ModeAsk))
	require.Equal(t, constants.ModeAsk, prevMode(constants.ModeBrave))
	require.Equal(t, constants.ModeBrave, prevMode("unknown"))
}

func TestInitReturnsCommands(t *testing.T) {
	m := New()
	cmd := m.Init()
	require.NotNil(t, cmd)
}

func TestAgentCommandsReturnMessages(t *testing.T) {
	actCmd := ActivityCmd(constants.ActivityWriting)
	require.Equal(t, constants.ActivityWriting, actCmd().(ActivityMsg).Activity)

	toolCmd := ActivityForToolCmd("read")
	require.NotEmpty(t, toolCmd().(ActivityMsg).Activity)

	doneCmd := AgentDoneCmd("response")
	require.Equal(t, "response", doneCmd().(AgentDoneMsg).Response)
}

func TestPlaceholderResponse(t *testing.T) {
	got := placeholderResponse("hello")
	require.Contains(t, got, "hello")
	require.Contains(t, got, "placeholder")
}

func TestEnableAndDisableTerminalFeatures(t *testing.T) {
	oldStdout := os.Stdout
	r, w, err := os.Pipe()
	require.NoError(t, err)
	os.Stdout = w
	t.Cleanup(func() {
		_ = w.Close()
		os.Stdout = oldStdout
		_, _ = io.Copy(io.Discard, r)
		_ = r.Close()
	})

	enable := enableTerminalFeatures()
	require.IsType(t, termFeaturesMsg{}, enable())

	disable := disableTerminalFeatures()
	require.Nil(t, disable())
	_ = w.Close()
}

func TestDecodeCSIByteList(t *testing.T) {
	require.Equal(t, "\x1b", decodeCSIByteList("27"))
	require.Equal(t, "\x1b[", decodeCSIByteList("27 91"))
	require.Equal(t, "\x1b\x02\r", decodeCSIByteList("27 2 13"))
	require.Empty(t, decodeCSIByteList("999"))
	require.Empty(t, decodeCSIByteList("abc"))

	require.Equal(t, "\x1b\x02\r", csiPayloadFromString(csiMsgForTest("[27 2 13]").String()))
	require.Equal(t, "27;2;13", csiPayloadFromString(csiMsgForTest("27;2;13").String()))
}

func TestStripTrigger(t *testing.T) {
	require.Equal(t, "ls", stripTrigger("!ls"))
	require.Equal(t, "cmd", stripTrigger("!!cmd"))
	require.Equal(t, "help", stripTrigger("/help"))
	require.Equal(t, "plain", stripTrigger("  plain"))
}

func TestViewStates(t *testing.T) {
	m := New()
	require.Equal(t, "\n  Initializing...", viewContent(m))

	m.ready = true
	m.width = 80
	m.height = 24
	m = m.syncLayout(false)
	require.NotEmpty(t, viewContent(m))

	m.quitting = true
	require.Empty(t, viewContent(m))
}

func TestClampAndWrapEdgeCases(t *testing.T) {
	require.Empty(t, clampLine(0, "hello"))
	require.Equal(t, "hello", wrapLine(0, "hello"))
}

func TestFooterGitColors(t *testing.T) {
	cases := []struct {
		added, deleted int
		wantSuffix     string
	}{
		{3, 0, "[+3 -0]"},
		{0, 2, "[+0 -2]"},
		{1, 1, "[+1 -1]"},
		{0, 0, "[-]"},
	}
	for _, tc := range cases {
		m := New()
		m.width = 80
		m.gitAdded = tc.added
		m.gitDeleted = tc.deleted
		footer := m.footerView()
		require.Contains(t, footer, tc.wantSuffix)
	}
}

func TestFooterRowRightDominant(t *testing.T) {
	longRight := "this right segment is intentionally very long indeed"
	row := footerRow(20, "left", longRight)
	require.LessOrEqual(t, lipgloss.Width(row), 20)
}

func TestIsInContentAreaEdgeCases(t *testing.T) {
	m := New()
	require.False(t, m.isInContentArea(0))

	m.ready = true
	m.content.SetHeight(10)
	require.True(t, m.isInContentArea(5))
	require.False(t, m.isInContentArea(10))
	require.False(t, m.isInContentArea(-1))
}

func TestShouldReleaseMouseForSelection(t *testing.T) {
	m := testInputModel(t)
	require.False(t, m.shouldReleaseMouseForSelection(mouseClick(1, 1, tea.MouseRight, 0)))
	require.False(t, m.shouldReleaseMouseForSelection(mouseWheel(1, 1, tea.MouseWheelDown)))

	m.mouseEnabled = false
	require.False(t, m.shouldReleaseMouseForSelection(mouseClick(1, 1, tea.MouseLeft, 0)))
}

func TestHandleMouseWhileSelecting(t *testing.T) {
	m := testInputModel(t)
	m.selectingText = true
	updated, cmds := m.handleMouse(mouseClick(1, 1, tea.MouseLeft, 0))
	require.True(t, updated.selectingText)
	require.Nil(t, cmds)
}

func TestBeginTextSelection(t *testing.T) {
	m := testInputModel(t)
	updated, cmds := m.beginTextSelection()
	require.False(t, updated.mouseEnabled)
	require.True(t, updated.selectingText)
	require.Len(t, cmds, 1)
}

func TestSyncInputChrome(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("line one\nline two")
	updated := m.syncInputChrome()
	require.GreaterOrEqual(t, updated.input.Height(), 2)
}

func TestOverlayInputScrollBarEdgeCases(t *testing.T) {
	body := "short"
	bar := ""
	require.Equal(t, body, overlayInputScrollBar(body, bar, lipgloss.Width(body)))

	// Short lines pad so the scrollbar sits on the right edge.
	got := overlayInputScrollBar("hi", "░", 10)
	require.Equal(t, 10, lipgloss.Width(got))
	require.True(t, strings.HasSuffix(stripANSI(got), "░"))
}

func TestIsShiftEnterKeyMsg(t *testing.T) {
	msg := keyEnter()
	// bubbletea may not expose shift+enter as a distinct type; test via String override path.
	require.False(t, isShiftEnterKeyMsg(msg))
}

func TestNormalizeInputEmpty(t *testing.T) {
	require.Empty(t, normalizeInputForSubmit(""))
}

func TestSyncLayoutNotReady(t *testing.T) {
	m := New()
	m.ready = false
	unchanged := m.syncLayout(true)
	require.False(t, unchanged.ready)
}

func TestSpinnerTickCmdWhenIdle(t *testing.T) {
	m := testInputModel(t)
	require.Nil(t, m.spinnerTickCmd())
}

func TestAgentTurnCmdsIncludesPhases(t *testing.T) {
	m := testInputModel(t)
	cmd := m.agentTurnCmds("test prompt")
	require.NotNil(t, cmd)
}
