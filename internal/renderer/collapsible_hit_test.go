package renderer

import (
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestMouseClickTogglesDetailBlockViaHintOnly(t *testing.T) {
	m := testModelWithLayout(t)
	m.height = 40
	m.messages = []message{
		{kind: uiconst.MessageDetail, detailLabel: "First", text: "alpha\nbeta"},
		{kind: uiconst.MessageDetail, detailLabel: "Second", text: "gamma\ndelta"},
	}
	m = m.syncLayout(false)

	// In compact format the hint is inline with the header, so clicking
	// the header line toggles the detail just as clicking the inline hint does.
	// Both header and footer resolve to the same row.
	headerY, ok := m.collapsibleHeaderViewportY(0)
	require.True(t, ok)
	updated, _ := m.Update(mouseClick(2, headerY, tea.MouseLeft, 0))
	m = updated.(Model)
	require.True(t, m.messages[0].detailExpanded)
	require.False(t, m.messages[1].detailExpanded)
	require.True(t, m.mouseEnabled)
	require.False(t, m.selectingText)
}

func TestMouseClickOnThinkingHeaderStillToggles(t *testing.T) {
	m := testModelWithLayout(t)
	m.messages = []message{
		{kind: uiconst.MessageThinking, detailLabel: "Thinking", text: "reasoning"},
		{kind: uiconst.MessageAI, text: "answer"},
	}
	m = m.syncLayout(false)

	y, ok := m.collapsibleHeaderViewportY(0)
	require.True(t, ok)

	updated, _ := m.Update(mouseClick(2, y, tea.MouseLeft, 0))
	m = updated.(Model)
	require.True(t, m.messages[0].detailExpanded)
}

func TestCtrlOTogglesNewestCollapsibleBlock(t *testing.T) {
	m := testModelWithLayout(t)
	m.height = 40
	m.messages = []message{
		{kind: uiconst.MessageDetail, detailLabel: "First", text: "alpha"},
		{kind: uiconst.MessageDetail, detailLabel: "Second", text: "beta"},
	}
	m = m.syncLayout(false)

	// In compact format the hint is inline with the header, so clicking
	// the header row toggles.
	headerY, ok := m.collapsibleHeaderViewportY(0)
	require.True(t, ok)
	updated, _ := m.Update(mouseClick(2, headerY, tea.MouseLeft, 0))
	m = updated.(Model)
	require.True(t, m.messages[0].detailExpanded)

	updated, _ = m.Update(keyCtrl('o'))
	m = updated.(Model)
	require.True(t, m.messages[0].detailExpanded)
	require.True(t, m.messages[1].detailExpanded)
}
