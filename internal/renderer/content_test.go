package renderer

import (
	"fmt"
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestContentViewIncludesBannerAndMessages(t *testing.T) {
	m := New()
	m.width = 80
	m.messages = []message{{text: "hello from user", kind: uiconst.MessageUser}}

	content := m.contentView()
	require.Contains(t, content, "Elph v")
	require.Contains(t, content, "hello from user")
}

func TestSyncLayoutFitsTerminalHeight(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 30
	m.ready = true

	m = m.syncLayout(false)

	require.Equal(t, m.width-scrollBarWidth, m.content.Width())
	require.Positive(t, m.content.Height())
	require.LessOrEqual(t, m.content.Height()+m.layout.ChromeH, m.height)
}

func TestContentViewLongPasteIncludesBannerOnce(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true

	readme := strings.Repeat("Elph minimalist AI agent companion. ", 80)
	m.messages = []message{{text: readme, kind: uiconst.MessageUser}}

	content := m.contentView()
	require.Equal(t, 1, strings.Count(content, "─ Elph"), "banner should appear once")
}

func TestBannerWidthMatchesTerminal(t *testing.T) {
	m := New()
	m.width = 50
	m.workDir = strings.Repeat("x", 80)

	banner := m.bannerView()
	require.LessOrEqual(t, lipgloss.Width(banner), m.width)
}

func TestManyMessagesViewportContent(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true

	for i := range 25 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("message %d", i),
			kind: uiconst.MessageUser,
		})
	}

	m = m.syncLayout(true)
	require.Contains(t, m.contentView(), "message 24")
	require.GreaterOrEqual(t, m.content.Height(), 1)
}
