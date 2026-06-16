package renderer

import (
	"errors"
	"github.com/riipandi/elph/internal/runtime/toolresult"
	"testing"

	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestToolDetailStatusTransitions(t *testing.T) {
	require.Equal(t, uiconst.DetailStatusSuccess, toolDetailStatus(toolresult.ToolResult{Output: "ok"}))
	require.Equal(t, uiconst.DetailStatusError, toolDetailStatus(toolresult.ToolResult{Err: errors.New("boom")}))
	require.Equal(t, uiconst.DetailStatusWarning, toolDetailStatus(toolresult.ToolResult{Cancelled: true}))
}

func TestToolDetailCollapsedByDefault(t *testing.T) {
	m := New()
	m = m.addToolDetailMessage("Bash", "echo hello")

	require.False(t, m.messages[0].detailExpanded)
	require.Contains(t, stripANSI(m.renderMessageAt(0)), "ctrl+o to expand")
}

func TestToolDetailShortContentCollapsedByDefault(t *testing.T) {
	m := New()
	m = m.addToolDetailMessage("Read", "file contents")

	require.False(t, m.messages[0].detailExpanded)
}

func TestToolDetailLongContentCollapsedByDefault(t *testing.T) {
	m := New()
	m = m.addToolDetailFromResult("Read", toolresult.ToolResult{
		Output: "line one\nline two\nline three",
	})

	require.False(t, m.messages[0].detailExpanded)
	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "ctrl+o to expand")
	require.NotContains(t, rendered, "line three")
}

func TestShellToolDetailLongContentCollapsedByDefault(t *testing.T) {
	m := New()
	m = m.addToolDetailMessage("$ go test ./", "line one\nline two\nline three")

	require.False(t, m.messages[0].detailExpanded)
	require.Contains(t, stripANSI(m.renderMessageAt(0)), "ctrl+o to expand")
}

func TestAddToolDetailFromResultFormatsFailure(t *testing.T) {
	m := New()
	m = m.addToolDetailFromResult("Read", toolresult.ToolResult{
		Output: "partial",
		Err:    errors.New("file not found"),
	})

	require.Len(t, m.messages, 1)
	require.Equal(t, uiconst.MessageDetail, m.messages[0].kind)
	require.Equal(t, "Read", m.messages[0].detailLabel)
	require.Equal(t, uiconst.DetailStatusError, m.messages[0].detailStatus)
	require.Contains(t, m.messages[0].text, "Tool failed")
	require.Contains(t, m.messages[0].text, "file not found")
	require.Contains(t, m.messages[0].text, "partial")
}

func TestToolDetailCollapsedShowsFailedPreview(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:         uiconst.MessageDetail,
		detailLabel:  "Read",
		text:         "Tool failed\n\nfile not found",
		detailStatus: uiconst.DetailStatusError,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Failed")
	require.NotContains(t, rendered, "file not found")
}

func TestToolDetailUnavailableStatus(t *testing.T) {
	m := New()
	m = m.addToolDetailFromResult("Read", toolresult.ToolResult{Err: toolresult.ErrToolUnavailable})

	require.Equal(t, uiconst.DetailStatusUnavailable, m.messages[0].detailStatus)
}

func TestToolDetailUnknownToolStatus(t *testing.T) {
	m := New()
	m = m.addToolDetailFromResult("McpFoo", toolresult.ToolResult{Err: toolresult.ErrToolUnknown})

	require.Equal(t, uiconst.DetailStatusError, m.messages[0].detailStatus)
}

func TestToolDetailExpandedShowsFailureBody(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:           uiconst.MessageDetail,
		detailLabel:    "Grep",
		text:           "Tool failed\n\npattern error",
		detailStatus:   uiconst.DetailStatusError,
		detailExpanded: true,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Tool failed")
	require.Contains(t, rendered, "pattern error")
}
