package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestDetailExpandedShowsAnimatedRunningPreview(t *testing.T) {
	m := testInputModel(t)
	m.agent.Busy = true
	m.agent.SpinnerFrame = 0
	m.messages = []message{{
		kind:           constants.MessageDetail,
		detailLabel:    "Bash",
		text:           "(running...)",
		detailStatus:   constants.DetailStatusRunning,
		detailExpanded: true,
	}}
	m = m.syncLayout(false)

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Running...")
	require.NotContains(t, rendered, "(running...)")

	updated, cmd := m.Update(spinnerTickMsg{})
	m = updated.(Model)
	require.NotNil(t, cmd)
	require.NotEqual(t, rendered, stripANSI(m.renderMessageAt(0)))
}

func TestDetailExpandedRunningShowsStreamedOutput(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:           constants.MessageDetail,
		detailLabel:    "$ echo hi",
		text:           "hi\n",
		detailStatus:   constants.DetailStatusRunning,
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
		kind:         constants.MessageDetail,
		detailLabel:  "$ ping 1.1.1.1",
		text:         "PING 1.1.1.1\n",
		detailStatus: constants.DetailStatusRunning,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "PING 1.1.1.1")
	require.NotContains(t, rendered, "Running...")
}

func TestDetailCollapsedShowsRunningStatusPreview(t *testing.T) {
	m := testModel()
	m.agent.SpinnerFrame = 2
	m.messages = []message{{
		kind:         constants.MessageDetail,
		detailLabel:  "$ ls",
		text:         "(running...)",
		detailStatus: constants.DetailStatusRunning,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Running...")
}

func TestDetailCollapsedShowsBodyPreviewWhenIdle(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:         constants.MessageDetail,
		detailLabel:  "$ ls",
		text:         "file.txt\nREADME.md",
		detailStatus: constants.DetailStatusSuccess,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "file.txt")
	require.NotContains(t, rendered, "Running...")
}

func TestThinkingCollapsedShowsSpinnerWhileAwaitingContent(t *testing.T) {
	m := testInputModel(t)
	m.agent.Busy = true
	m.agent.SpinnerFrame = 1
	m.messages = []message{{
		kind:        constants.MessageThinking,
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
		kind:           constants.MessageThinking,
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

func TestThinkingExpandedEmptyShowsSpinnerWhileStreaming(t *testing.T) {
	m := testInputModel(t)
	m.agent.Busy = true
	m.agent.SpinnerFrame = 1
	m.messages = []message{{
		kind:           constants.MessageThinking,
		detailLabel:    "Thinking",
		detailExpanded: true,
	}}
	m.agent.ThinkingMsgID = 0

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Thinking...")
}

func TestThinkingCollapsedShowsPreviewWhileStreaming(t *testing.T) {
	m := testInputModel(t)
	m.agent.Busy = true
	m.agent.SpinnerFrame = 1
	m = m.addThinkingMessage("reasoning step one\nreasoning step two")
	m.agent.ThinkingMsgID = 0

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Thinking")
	require.Contains(t, rendered, "reasoning step one")
	require.NotContains(t, rendered, "reasoning step two")
	require.Contains(t, rendered, "click or ctrl+o to expand")
}

func TestThinkingCollapsedShowsPreviewWhileResponseStreams(t *testing.T) {
	m := testInputModel(t)
	m.width = 80
	m.agent.Busy = true
	m.messages = []message{
		{text: "prompt", kind: constants.MessageUser},
		{text: "reasoning in flight", kind: constants.MessageThinking, detailLabel: "Thinking"},
		{text: "answer so far", kind: constants.MessageAI},
	}
	m.agent.ThinkingMsgID = 1
	m.agent.ResponseMsgID = 2

	rendered := stripANSI(m.renderMessageAt(1))
	require.Contains(t, rendered, "Thinking")
	require.Contains(t, rendered, "reasoning in flight")
	require.Contains(t, rendered, "click or ctrl+o to expand")
}

func TestThinkingCollapsedShowsBodyWhenNotStreaming(t *testing.T) {
	m := testInputModel(t)
	m = m.addThinkingMessage("reasoning step one\nreasoning step two")

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "reasoning step one")
	require.NotContains(t, rendered, "Thinking...")
}

func TestStatusPreviewInsideColoredDetailBox(t *testing.T) {
	m := testModel()
	m.agent.SpinnerFrame = 0
	m.messages = []message{{
		kind:         constants.MessageDetail,
		detailLabel:  "$ ls",
		text:         "(running...)",
		detailStatus: constants.DetailStatusRunning,
	}}

	rendered := m.renderMessageAt(0)
	require.Contains(t, rendered, "\x1b[48", "detail box should keep status background")
	require.Contains(t, stripANSI(rendered), "Running...")

	boxStyle := constants.DetailStatusStyle(constants.DetailStatusRunning)
	preview := collapsibleStatusPreview(constants.MessageDetail, constants.DetailStatusRunning, boxStyle, 0, 80)
	require.Contains(t, preview, "48;2;40;40;50", "status text should inherit running detail box background")
	require.NotContains(t, preview, "49m", "status text should not reset to parent background")
	require.NotRegexp(t, `\x1b\[m `, preview, "gap between spinner and label should not be unstyled")
}

func TestThinkingStatusPreviewEllipsisHasBoxBackground(t *testing.T) {
	boxStyle := constants.MessageStyle(constants.MessageThinking).Italic(true)
	preview := collapsibleStatusPreview(constants.MessageThinking, constants.DetailStatusNeutral, boxStyle, 0, 80)
	require.Contains(t, preview, "Thinking...")
	require.Contains(t, preview, "48;2;35;35;35", "ellipsis should inherit thinking box background")
	require.NotRegexp(t, `Thinking\x1b\[m`, preview, "style should not reset before ellipsis")
}

func TestStatusPreviewTruncationEllipsisHasBoxBackground(t *testing.T) {
	boxStyle := constants.DetailStatusStyle(constants.DetailStatusRunning)
	preview := collapsibleStatusPreview(constants.MessageDetail, constants.DetailStatusRunning, boxStyle, 0, 12)
	require.Contains(t, stripANSI(preview), "...")
	require.Contains(t, preview, "48;2;40;40;50", "truncation ellipsis should inherit box background")
}

func TestThinkingHeaderChevronHasChipBackground(t *testing.T) {
	style := constants.MessageStyle(constants.MessageThinking).Italic(true)
	chip := collapsibleHeaderChip(style, constants.MessageThinking, "Thinking", false)
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
		kind:         constants.MessageDetail,
		detailLabel:  "$ sleep 1",
		text:         "(running...)",
		detailStatus: constants.DetailStatusRunning,
	}}
	m = m.syncLayout(false)

	before := m.renderMessageAt(0)
	updated, cmd := m.Update(spinnerTickMsg{})
	m = updated.(Model)
	require.NotNil(t, cmd)
	after := m.renderMessageAt(0)
	require.NotEqual(t, before, after)
}
