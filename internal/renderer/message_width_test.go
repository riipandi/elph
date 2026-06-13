package renderer

import (
	"fmt"
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestUserMessageLinesFitContentViewportWithScrollbar(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 14
	m.ready = true
	for i := range 25 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("user message %d with enough text to matter", i),
			kind: constants.MessageUser,
		})
	}
	m.contentDirty = true
	m = m.syncLayout(false)

	require.True(t, m.contentScrollable())

	content := m.contentView()
	maxLineW := 0
	for _, line := range strings.Split(content, "\n") {
		if w := lipgloss.Width(line); w > maxLineW {
			maxLineW = w
		}
	}
	require.LessOrEqual(t, maxLineW, m.content.Width, "content line width vs viewport")

	userW := lipgloss.Width(m.renderMessage(m.messages[len(m.messages)-1]))
	msgW := m.messageAreaWidth()
	require.Equal(t, msgW, userW)
	require.Less(t, userW, m.content.Width, "user message should be narrower than viewport")
	require.LessOrEqual(t, lipgloss.Width(m.contentAreaView()), m.width)
}