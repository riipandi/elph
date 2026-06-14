package renderer

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func boxedMessageKind(kind constants.MessageKind) bool {
	return kind == constants.MessageUser || kind == constants.MessageTool
}

func expectedBlankLinesBetween(prev, curr constants.MessageKind) int {
	blanks := 1 // messageBlockGap
	if boxedMessageKind(prev) {
		blanks++
	}
	if boxedMessageKind(curr) {
		blanks++
	}
	if prev == constants.MessageAI {
		blanks++ // bottom padding inside AI block
	}
	if prev == constants.MessageDetail || prev == constants.MessageThinking {
		blanks += 6
	}
	return blanks
}

func TestMessageSpacingMatrixConsistent(t *testing.T) {
	kinds := []struct {
		name string
		kind constants.MessageKind
		text string
	}{
		{"thinking", constants.MessageThinking, "[[thinking]]"},
		{"ai", constants.MessageAI, "[[ai]]"},
		{"user", constants.MessageUser, "[[user]]"},
		{"detail", constants.MessageDetail, "detail body"},
		{"tool", constants.MessageTool, "[[tool]]"},
		{"system", constants.MessageSystem, "[[system]]"},
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
		promptSpacingMessage("[[think]]", constants.MessageThinking),
		{text: "[[answer]]", kind: constants.MessageAI},
		{text: "[[prompt]]", kind: constants.MessageUser},
		{text: "[[reply]]", kind: constants.MessageAI},
		{text: "[[shell]]", kind: constants.MessageTool},
		{text: "[[note]]", kind: constants.MessageSystem},
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

func spacingMarker(text string, kind constants.MessageKind) string {
	if kind == constants.MessageDetail || kind == constants.MessageThinking {
		return "[[collapsible-block]]"
	}
	return text
}

func promptSpacingMessage(text string, kind constants.MessageKind) message {
	msg := message{text: text, kind: kind}
	if kind == constants.MessageDetail || kind == constants.MessageThinking {
		msg.detailLabel = "[[collapsible-block]]"
	}
	return msg
}

func kindForMarker(marker string) constants.MessageKind {
	switch marker {
	case "[[think]]":
		return constants.MessageThinking
	case "[[answer]]", "[[reply]]":
		return constants.MessageAI
	case "[[prompt]]":
		return constants.MessageUser
	case "[[shell]]":
		return constants.MessageTool
	case "[[note]]":
		return constants.MessageSystem
	default:
		return constants.MessageAI
	}
}

func TestActiveTurnMessageSpacingConsistent(t *testing.T) {
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
	updated, _ = m.Update(agentEventMsg{event: agent.ResponseDeltaEvent("[[answer]]")})
	m = updated.(Model)

	content := normalizeSpacingLines(stripANSI(m.messagesView()))
	require.Contains(t, content, "[[think]]")
	require.Contains(t, content, "[[answer]]")
	require.Equal(t, expectedBlankLinesBetween(constants.MessageUser, constants.MessageThinking),
		blankLinesBetweenMarkers(content, formatMessageTimestamp(m.messages[0].at), "Thinking"))
	require.Equal(t, expectedBlankLinesBetween(constants.MessageThinking, constants.MessageAI),
		blankLinesBetweenMarkers(content, "Thinking", "[[answer]]"))
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
