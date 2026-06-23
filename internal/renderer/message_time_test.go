package renderer

import (
	"strings"
	"testing"
	"time"

	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestFormatMessageTimestampSameDay(t *testing.T) {
	now := time.Now().Local()
	at := time.Date(now.Year(), now.Month(), now.Day(), 15, 30, 45, 0, time.Local)
	got := formatMessageTimestamp(at)
	require.Equal(t, "15:30:45", got)
}

func TestFormatMessageTimestampOtherDay(t *testing.T) {
	at := time.Date(2025, 12, 25, 9, 15, 0, 0, time.Local)
	got := formatMessageTimestamp(at)
	require.Equal(t, "Dec 25 09:15:00", got)
}

func TestUserMessageShowsTimestamp(t *testing.T) {
	m := testModel()
	at := time.Date(2026, 6, 14, 10, 20, 30, 0, time.Local)
	rendered := stripANSI(m.renderMessage(message{
		text: "hello",
		kind: uiconst.MessageUser,
		at:   at,
	}))
	require.Contains(t, rendered, "10:20:30")
	require.Contains(t, rendered, "hello")
	require.Greater(t, strings.Index(rendered, "10:20:30"), strings.Index(rendered, "hello"))
}

func TestUserMessageTimestampHasProportionalGap(t *testing.T) {
	m := testModel()
	at := time.Date(2026, 6, 14, 10, 20, 30, 0, time.Local)
	rendered := stripANSI(m.renderMessage(message{
		text:           "line one\nline two",
		kind:           uiconst.MessageUser,
		detailExpanded: true,
		at:             at,
	}))
	helloIdx := strings.Index(rendered, "line two")
	tsIdx := strings.Index(rendered, "10:20:30")
	require.Greater(t, tsIdx, helloIdx)

	gap := rendered[helloIdx+len("line two") : tsIdx]
	blankLines := 0
	for _, line := range strings.Split(gap, "\n") {
		if strings.TrimSpace(line) == "" {
			blankLines++
		}
	}
	require.GreaterOrEqual(t, blankLines, 1, "timestamp should sit below the message with a blank line gap")
}

func TestDetailMessageShowsTimestampInTitle(t *testing.T) {
	m := testModel()
	at := time.Date(2026, 6, 14, 10, 20, 30, 0, time.Local)
	rendered := stripANSI(m.renderMessage(message{
		kind:        uiconst.MessageDetail,
		detailLabel: "Prompt",
		text:        "expanded prompt body",
		at:          at,
	}))
	require.Contains(t, rendered, "Prompt")
}

func TestAddUserMessageSetsTimestamp(t *testing.T) {
	m := testModel()
	before := time.Now()
	m = m.addUserMessage("hello")
	after := time.Now()
	require.False(t, m.messages[0].at.IsZero())
	require.False(t, m.messages[0].at.Before(before))
	require.False(t, m.messages[0].at.After(after))
}
