package renderer

import (
	"errors"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/runtime"
	"github.com/stretchr/testify/require"
)

func TestToolDetailStatusTransitions(t *testing.T) {
	require.Equal(t, constants.DetailStatusSuccess, toolDetailStatus(runtime.ToolResult{Output: "ok"}))
	require.Equal(t, constants.DetailStatusError, toolDetailStatus(runtime.ToolResult{Err: errors.New("boom")}))
	require.Equal(t, constants.DetailStatusWarning, toolDetailStatus(runtime.ToolResult{Cancelled: true}))
}

func TestAddToolDetailFromResultFormatsFailure(t *testing.T) {
	m := New()
	m = m.addToolDetailFromResult("Read", runtime.ToolResult{
		Output: "partial",
		Err:    errors.New("file not found"),
	})

	require.Len(t, m.messages, 1)
	require.Equal(t, constants.MessageDetail, m.messages[0].kind)
	require.Equal(t, "Read", m.messages[0].detailLabel)
	require.Equal(t, constants.DetailStatusError, m.messages[0].detailStatus)
	require.Contains(t, m.messages[0].text, "Tool failed")
	require.Contains(t, m.messages[0].text, "file not found")
	require.Contains(t, m.messages[0].text, "partial")
}

func TestToolDetailCollapsedShowsFailedPreview(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:         constants.MessageDetail,
		detailLabel:  "Read",
		text:         "Tool failed\n\nfile not found",
		detailStatus: constants.DetailStatusError,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Failed")
	require.NotContains(t, rendered, "file not found")
}

func TestToolDetailUnavailableStatus(t *testing.T) {
	m := New()
	m = m.addToolDetailFromResult("Read", runtime.ToolResult{Err: runtime.ErrToolUnavailable})

	require.Equal(t, constants.DetailStatusUnavailable, m.messages[0].detailStatus)
}

func TestToolDetailUnknownToolStatus(t *testing.T) {
	m := New()
	m = m.addToolDetailFromResult("McpFoo", runtime.ToolResult{Err: runtime.ErrToolUnknown})

	require.Equal(t, constants.DetailStatusError, m.messages[0].detailStatus)
}

func TestToolDetailExpandedShowsFailureBody(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:           constants.MessageDetail,
		detailLabel:    "Grep",
		text:           "Tool failed\n\npattern error",
		detailStatus:   constants.DetailStatusError,
		detailExpanded: true,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Tool failed")
	require.Contains(t, rendered, "pattern error")
}
