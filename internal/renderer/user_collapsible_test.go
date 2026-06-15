package renderer

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestUserMessageCollapsedByDefault(t *testing.T) {
	m := testModel()
	m = m.addUserMessage("line one\nline two")
	require.False(t, m.messages[0].detailExpanded)

	rendered := stripANSI(m.renderMessage(m.messages[0]))
	require.Contains(t, rendered, "▸")
	require.Contains(t, rendered, "line one")
	require.NotContains(t, rendered, "line two")
	require.Contains(t, rendered, "click or ctrl+o to expand")
}

func TestUserMessageSingleLineNoHintOrChevron(t *testing.T) {
	m := testModel()
	at := time.Date(2026, 6, 14, 10, 20, 30, 0, time.Local)
	rendered := stripANSI(m.renderMessage(message{
		text: "hello",
		kind: constants.MessageUser,
		at:   at,
	}))
	require.Contains(t, rendered, "hello")
	require.Contains(t, rendered, "10:20:30")
	require.NotContains(t, rendered, "▸")
	require.NotContains(t, rendered, "click or ctrl+o")
}

func TestUserMessageFooterTimestampBeforeHint(t *testing.T) {
	m := testModel()
	at := time.Date(2026, 6, 14, 10, 20, 30, 0, time.Local)
	rendered := stripANSI(m.renderMessage(message{
		text: "hello\nworld",
		kind: constants.MessageUser,
		at:   at,
	}))
	tsIdx := strings.Index(rendered, "10:20:30")
	hintIdx := strings.Index(rendered, "click or ctrl+o to expand")
	require.Greater(t, hintIdx, tsIdx)
	require.Greater(t, tsIdx, strings.Index(rendered, "hello"))
}

func TestUserMessageExpandShowsFullContent(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		text:           "line one\nline two",
		kind:           constants.MessageUser,
		detailExpanded: true,
		at:             time.Now(),
	}}

	rendered := stripANSI(m.renderMessage(m.messages[0]))
	require.Contains(t, rendered, "line one")
	require.Contains(t, rendered, "line two")
	require.Contains(t, rendered, "click or ctrl+o to collapse")
	require.Greater(t, lipgloss.Height(m.renderMessage(m.messages[0])), 3)
}

func TestAddUserMessageStartsCollapsed(t *testing.T) {
	m := testModel()
	m = m.addUserMessage("hello")
	require.False(t, m.messages[0].detailExpanded)
}

func TestSingleLineUserMessageNotCollapsible(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		text: "hello",
		kind: constants.MessageUser,
		at:   time.Now(),
	}}
	_, toggled := m.toggleDetailExpandAt(0)
	require.False(t, toggled)
}

func TestMouseClickTogglesUserMessageFooter(t *testing.T) {
	m := testModelWithLayout(t)
	m.height = 40
	m.messages = []message{{
		text: "alpha\nbeta",
		kind: constants.MessageUser,
		at:   time.Now(),
	}}
	m = m.syncLayout(false)

	footerY, ok := m.collapsibleFooterViewportY(0)
	require.True(t, ok)
	updated, cmds := m.Update(mouseClick(2, footerY, tea.MouseLeft, 0))
	m = updated.(Model)
	require.Empty(t, cmds)
	require.True(t, m.messages[0].detailExpanded)
}

func TestCtrlOTogglesUserMessageExpand(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		text: "alpha\nbeta",
		kind: constants.MessageUser,
		at:   time.Now(),
	}}

	m, toggled := m.toggleDetailExpandAt(0)
	require.True(t, toggled)
	require.True(t, m.messages[0].detailExpanded)

	rendered := stripANSI(m.renderMessage(m.messages[0]))
	require.Contains(t, rendered, "alpha")
	require.Contains(t, rendered, "beta")
}
