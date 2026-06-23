package renderer

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func boxedMessageKind(kind uiconst.MessageKind) bool {
	return kind == uiconst.MessageUser || kind == uiconst.MessageTool
}

func expectedBlankLinesBetween(prev, curr uiconst.MessageKind) int {
	blanks := 1 // messageBlockGap
	if boxedMessageKind(prev) {
		blanks++
	}
	if boxedMessageKind(curr) {
		blanks++
	}
	if prev == uiconst.MessageAI {
		blanks++    // bottom padding inside AI block
		blanks += 2 // copy hint separator before footer line
	}
	return blanks
}

func TestMessageSpacingMatrixConsistent(t *testing.T) {
	kinds := []struct {
		name string
		kind uiconst.MessageKind
		text string
	}{
		{"thinking", uiconst.MessageThinking, "[[thinking]]"},
		{"ai", uiconst.MessageAI, "[[ai]]"},
		{"user", uiconst.MessageUser, "[[user]]"},
		{"detail", uiconst.MessageDetail, "detail body"},
		{"tool", uiconst.MessageTool, "[[tool]]"},
		{"system", uiconst.MessageSystem, "[[system]]"},
	}

	for _, prev := range kinds {
		for _, curr := range kinds {
			m := testModel()
			m.messages = []message{
				promptSpacingMessage(prev.text, prev.kind),
				promptSpacingMessage(curr.text, curr.kind),
			}
			content := normalizeSpacingLines(stripANSI(m.messagesView()))
			blanks := blankLinesBetweenMarkers(
				content,
				spacingMarker(prev.text, prev.kind),
				spacingMarker(curr.text, curr.kind),
			)
			want := expectedBlankLinesBetween(prev.kind, curr.kind)
			require.Equal(t, want, blanks, "%s -> %s", prev.name, curr.name)
		}
	}
}

func TestAssistantTurnSpacingConsistent(t *testing.T) {
	m := testModel()
	m.messages = []message{
		promptSpacingMessage("[[think]]", uiconst.MessageThinking),
		{text: "[[answer]]", kind: uiconst.MessageAI},
		{text: "[[prompt]]", kind: uiconst.MessageUser},
		{text: "[[reply]]", kind: uiconst.MessageAI},
		{text: "[[shell]]", kind: uiconst.MessageTool},
		{text: "[[note]]", kind: uiconst.MessageSystem},
	}
	content := normalizeSpacingLines(stripANSI(m.messagesView()))

	for i := 1; i < len(m.messages); i++ {
		prev, curr := m.messages[i-1], m.messages[i]
		left := spacingMarker(prev.text, prev.kind)
		right := spacingMarker(curr.text, curr.kind)
		blanks := blankLinesBetweenMarkers(content, left, right)
		want := expectedBlankLinesBetween(prev.kind, curr.kind)
		require.Equal(t, want, blanks, "%s -> %s", left, right)
	}
}

func spacingMarker(text string, kind uiconst.MessageKind) string {
	switch kind {
	case uiconst.MessageThinking:
		return "Thought" // compact format always shows "Thought" header
	case uiconst.MessageDetail:
		return "[[collapsible-block]]" // label always in header
	default:
		return text
	}
}

func promptSpacingMessage(text string, kind uiconst.MessageKind) message {
	msg := message{text: text, kind: kind}
	if kind == uiconst.MessageDetail || kind == uiconst.MessageThinking {
		msg.detailLabel = "[[collapsible-block]]"
	}
	return msg
}

//nolint:unused // kept for future use
func kindForMarker(marker string) uiconst.MessageKind {
	switch marker {
	case "[[think]]":
		return uiconst.MessageThinking
	case "[[answer]]", "[[reply]]":
		return uiconst.MessageAI
	case "[[prompt]]":
		return uiconst.MessageUser
	case "[[shell]]":
		return uiconst.MessageTool
	case "[[note]]":
		return uiconst.MessageSystem
	default:
		return uiconst.MessageAI
	}
}

func TestActiveTurnMessageSpacingConsistent(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	m := testModel()
	m.height = 24
	m.ready = true
	m = m.syncLayout(false)

	m.input.SetValue("[[prompt]]")
	updated, _ := m.Update(keyEnter())
	m = updated.(Model)
	require.True(t, m.showsActivity())
	updated, _ = m.Update(agentEventMsg{event: agent.ThinkingDeltaEvent("[[think]]")})
	m = updated.(Model)
	// Collapse thinking for consistent spacing measurement
	if tIdx := m.agent.ThinkingMsgID; tIdx >= 0 && tIdx < len(m.messages) {
		m.messages[tIdx].detailExpanded = false
	}
	updated, _ = m.Update(agentEventMsg{event: agent.ResponseDeltaEvent("[[answer]]")})
	m = updated.(Model)

	content := normalizeSpacingLines(stripANSI(m.messagesView()))
	require.Contains(t, content, "[[answer]]")
	// Spacing is measured from the user footer timestamp, which already sits below the prompt body.
	require.Equal(t, 2,
		blankLinesBetweenMarkers(content, formatMessageTimestamp(m.messages[0].at), "Thought"))
	require.Equal(t, expectedBlankLinesBetween(uiconst.MessageThinking, uiconst.MessageAI),
		blankLinesBetweenMarkers(content, "Thought", "[[answer]]"))
}

func TestActivityChromeGapMatchesIdleInputMargin(t *testing.T) {
	m := testModel()
	m.height = 24
	m.ready = true
	idle := m.syncLayout(false)

	idleGap := lipgloss.Height(idle.inputChromeView()) - lipgloss.Height(idle.inputBoxView(false))
	require.Equal(t, 1, idleGap)

	busy := idle.beginAgentTurn().syncLayout(true)
	activityGap := lipgloss.Height(busy.activityView()) - 1
	require.Equal(t, idleGap, activityGap, "gap above activity should match idle input top margin")
	require.Equal(t, idle.layout.ChromeH+1, busy.layout.ChromeH)
}

func blankLinesBetweenMarkers(content, left, right string) int {
	leftIdx := strings.Index(content, left)
	if leftIdx < 0 {
		return -1
	}
	afterLeft := leftIdx + len(left)
	rightIdx := strings.Index(content[afterLeft:], right)
	if rightIdx < 0 {
		return -1
	}
	segment := content[afterLeft : afterLeft+rightIdx]
	if segment == "" {
		return 0
	}
	return strings.Count(segment, "\n") - 1
}
