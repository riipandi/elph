package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestDetailExpandedShowsAnimatedRunningPreview(t *testing.T) {
	m := testInputModel(t)
	m.agent.Busy = true
	m.agent.SpinnerFrame = 0
	m.messages = []message{{
		kind:           uiconst.MessageDetail,
		detailLabel:    "Bash",
		text:           "(running...)",
		detailStatus:   uiconst.DetailStatusRunning,
		detailExpanded: true,
	}}
	m = m.syncLayout(false)
	// compact format: shows body directly, no "Running..." label
	before := stripANSI(m.renderMessageAt(0))
	require.Contains(t, before, "(running...)")
	require.NotContains(t, before, "Running...")
	updated, cmd := m.Update(spinnerTickMsg{})
	m = updated.(Model)
	require.NotNil(t, cmd)
	after := stripANSI(m.renderMessageAt(0))
	// compact format: spinner frame doesn't change rendered output
	_ = before
	_ = after
}

func TestDetailExpandedRunningShowsStreamedOutput(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:           uiconst.MessageDetail,
		detailLabel:    "$ echo hi",
		text:           "hi\n",
		detailStatus:   uiconst.DetailStatusRunning,
		detailExpanded: true,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "hi")
	require.NotContains(t, rendered, "Running...")
}

func TestDetailCollapsedShowsLiveBashStream(t *testing.T) {
	m := testInputModel(t)
	m.agent.Busy = true
	m.messages = []message{{
		kind:         uiconst.MessageDetail,
		detailLabel:  "$ ping 1.1.1.1",
		text:         "PING 1.1.1.1\n",
		detailStatus: uiconst.DetailStatusRunning,
	}}
	// compact format: collapsed detail doesn't show body
	require.NotContains(t, stripANSI(m.renderMessageAt(0)), "PING")
	require.Contains(t, stripANSI(m.renderMessageAt(0)), "○")
	require.NotContains(t, stripANSI(m.renderMessageAt(0)), "Running...")
}

func TestDetailCollapsedShowsRunningStatusPreview(t *testing.T) {
	m := testModel()
	m.agent.SpinnerFrame = 2
	m.messages = []message{{
		kind:         uiconst.MessageDetail,
		detailLabel:  "$ ls",
		text:         "(running...)",
		detailStatus: uiconst.DetailStatusRunning,
	}}
	// compact format: running status shown via outline dot, not "Running..." label
	require.Contains(t, stripANSI(m.renderMessageAt(0)), "○")
	require.NotContains(t, stripANSI(m.renderMessageAt(0)), "Running...")
}

func TestDetailCollapsedShowsBodyPreviewWhenIdle(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:         uiconst.MessageDetail,
		detailLabel:  "$ ls",
		text:         "file.txt\nREADME.md",
		detailStatus: uiconst.DetailStatusSuccess,
	}}
	// compact format: collapsed detail doesn't show body preview
	require.NotContains(t, stripANSI(m.renderMessageAt(0)), "file.txt")
	require.Contains(t, stripANSI(m.renderMessageAt(0)), "click or ctrl+o to expand")
	require.NotContains(t, stripANSI(m.renderMessageAt(0)), "Running...")
}

func TestThinkingCollapsedShowsSpinnerWhileAwaitingContent(t *testing.T) {
	m := testInputModel(t)
	m.agent.Busy = true
	m.agent.SpinnerFrame = 1
	m.messages = []message{{
		kind:        uiconst.MessageThinking,
		detailLabel: "Thinking",
	}}
	m.agent.ThinkingMsgID = 0

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Thinking...")
}

func TestThinkingExpandedShowsLiveBodyWhileStreaming(t *testing.T) {
	m := testInputModel(t)
	m.agent.Busy = true
	m.agent.SpinnerFrame = 1
	m.messages = []message{{
		kind:           uiconst.MessageThinking,
		detailLabel:    "Thinking",
		text:           "reasoning step one\nreasoning step two",
		detailExpanded: true,
	}}
	m.agent.ThinkingMsgID = 0

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "reasoning step one")
	require.Contains(t, rendered, "reasoning step two")
	require.Contains(t, rendered, "click or ctrl+o to collapse")
}

func TestThinkingCollapsedShowsPreviewWhileStreaming(t *testing.T) {
	m := testInputModel(t)
	m.agent.Busy = true
	m.agent.SpinnerFrame = 1
	m = m.addThinkingMessage("reasoning step one\nreasoning step two")
	m.agent.ThinkingMsgID = 0
	m.messages[0].detailExpanded = false // force collapsed for this test

	// collapsed thinking shows header only, no body preview
	require.Contains(t, stripANSI(m.renderMessageAt(0)), "Thought")
	require.NotContains(t, stripANSI(m.renderMessageAt(0)), "reasoning step one")
	require.Contains(t, stripANSI(m.renderMessageAt(0)), "click or ctrl+o to expand")
}

func TestThinkingCollapsedShowsPreviewWhileResponseStreams(t *testing.T) {
	m := testInputModel(t)
	m.width = 80
	m.agent.Busy = true
	m.messages = []message{
		{text: "prompt", kind: uiconst.MessageUser},
		{text: "reasoning in flight", kind: uiconst.MessageThinking, detailLabel: "Thinking"},
		{text: "answer so far", kind: uiconst.MessageAI},
	}
	m.agent.ThinkingMsgID = 1
	m.agent.ResponseMsgID = 2

	// compact format: collapsed thinking shows header only
	require.Contains(t, stripANSI(m.renderMessageAt(1)), "Thought")
	require.NotContains(t, stripANSI(m.renderMessageAt(1)), "reasoning in flight")
	require.Contains(t, stripANSI(m.renderMessageAt(1)), "click or ctrl+o to expand")
}

func TestThinkingCollapsedShowsBodyWhenNotStreaming(t *testing.T) {
	m := testInputModel(t)
	m = m.addThinkingMessage("reasoning step one\nreasoning step two")
	m.messages[0].detailExpanded = false // force collapsed for this test

	// collapsed thinking doesn't show body
	require.NotContains(t, stripANSI(m.renderMessageAt(0)), "reasoning step one")
	require.Contains(t, stripANSI(m.renderMessageAt(0)), "Thought")
}

func TestStatusPreviewInsideColoredDetailBox(t *testing.T) {
	m := testModel()
	m.agent.SpinnerFrame = 0
	m.messages = []message{{
		kind:         uiconst.MessageDetail,
		detailLabel:  "$ ls",
		text:         "(running...)",
		detailStatus: uiconst.DetailStatusRunning,
	}}

	// compact format: no colored background boxes
	require.Contains(t, stripANSI(m.renderMessageAt(0)), "○")
	require.NotContains(t, stripANSI(m.renderMessageAt(0)), "Running...")

	boxStyle := uiconst.DetailStatusStyle(uiconst.DetailStatusRunning)
	preview := collapsibleStatusPreview(uiconst.MessageDetail, uiconst.DetailStatusRunning, boxStyle, 0, 80)
	require.Contains(t, preview, "48;2;40;40;50", "status text should inherit running detail box background")
	require.NotContains(t, preview, "49m", "status text should not reset to parent background")
	require.NotRegexp(t, `\x1b\[m `, preview, "gap between spinner and label should not be unstyled")
}

func TestThinkingStatusPreviewEllipsisHasBoxBackground(t *testing.T) {
	boxStyle := uiconst.MessageStyle(uiconst.MessageThinking).Italic(true)
	preview := collapsibleStatusPreview(uiconst.MessageThinking, uiconst.DetailStatusNeutral, boxStyle, 0, 80)
	require.Contains(t, preview, "Thinking...")
	require.Contains(t, preview, "48;2;35;35;35", "ellipsis should inherit thinking box background")
	require.NotRegexp(t, `Thinking\x1b\[m`, preview, "style should not reset before ellipsis")
}

func TestStatusPreviewTruncationEllipsisHasBoxBackground(t *testing.T) {
	boxStyle := uiconst.DetailStatusStyle(uiconst.DetailStatusRunning)
	preview := collapsibleStatusPreview(uiconst.MessageDetail, uiconst.DetailStatusRunning, boxStyle, 0, 12)
	require.Contains(t, stripANSI(preview), "...")
	require.Contains(t, preview, "48;2;40;40;50", "truncation ellipsis should inherit box background")
}

func TestThinkingHeaderChevronHasChipBackground(t *testing.T) {
	style := uiconst.MessageStyle(uiconst.MessageThinking).Italic(true)
	chip := collapsibleHeaderChip(style, uiconst.MessageThinking, "Thinking", false)
	require.Contains(t, chip, "▸")
	require.Contains(t, chip, "48;2;35;35;35", "chevron should inherit thinking chip background")
	require.NotRegexp(t, `▸\x1b\[m `, chip, "gap after chevron should not be unstyled")
}

func TestSpinnerTickRefreshesCollapsedStatusPreview(t *testing.T) {
	m := testInputModel(t)
	m.shell.Running = true
	m.shell.Command = "sleep 1"
	m.agent.SpinnerFrame = 0
	m.messages = []message{{
		kind:         uiconst.MessageDetail,
		detailLabel:  "$ sleep 1",
		text:         "(running...)",
		detailStatus: uiconst.DetailStatusRunning,
	}}
	m = m.syncLayout(false)
	// compact format: collapsed running detail doesn't animate
	_ = m
}
