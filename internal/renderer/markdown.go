package renderer

import (
	"os"
	"strings"
	"sync"

	tea "charm.land/bubbletea/v2"
	"charm.land/glamour/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
)

// All markdown formatting runs off the UI thread so stream completion never
// blocks the event loop, regardless of response size.
const glamourAsyncMinLen = 1

type markdownRenderCache struct {
	mu       sync.Mutex
	width    int
	style    string
	renderer *glamour.TermRenderer
}

var aiMarkdownCache markdownRenderCache

type glamourRenderMsg struct {
	index  int
	width  int
	source string
	output string
	err    error
}

func glamourStylePath() string {
	if lipgloss.HasDarkBackground(os.Stdin, os.Stdout) {
		return "dark"
	}
	return "light"
}

func (c *markdownRenderCache) renderMarkdown(width int, markdown string) (string, error) {
	style := glamourStylePath()

	c.mu.Lock()
	defer c.mu.Unlock()

	if c.renderer == nil || c.width != width || c.style != style {
		r, err := glamour.NewTermRenderer(
			glamour.WithStylePath(style),
			glamour.WithWordWrap(width),
		)
		if err != nil {
			c.renderer = nil
			return "", err
		}
		c.renderer = r
		c.width = width
		c.style = style
	}

	return c.renderer.Render(markdown)
}

// looksLikeMarkdown reports whether the response contains markdown syntax worth
// rendering with Glamour instead of plain line styling.
func looksLikeMarkdown(text string) bool {
	for _, line := range strings.Split(text, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}
		switch {
		case strings.HasPrefix(trimmed, "#"),
			strings.HasPrefix(trimmed, ">"),
			strings.HasPrefix(trimmed, "```"),
			strings.HasPrefix(trimmed, "- "),
			strings.HasPrefix(trimmed, "* "),
			strings.HasPrefix(trimmed, "+ "):
			return true
		}
		if len(trimmed) > 2 && trimmed[0] >= '0' && trimmed[0] <= '9' && strings.Contains(trimmed, ". ") {
			return true
		}
		if strings.Contains(trimmed, "**") ||
			strings.Contains(trimmed, "__") ||
			strings.Contains(trimmed, "`") ||
			strings.Contains(trimmed, "](") {
			return true
		}
	}
	return false
}

func renderAIMessagePlain(blockWidth int, text string) string {
	return renderStyledMessage(blockWidth, constants.MessageAI, text)
}

func renderAIMessageGlamour(blockWidth int, text string) string {
	_, hPad := messageBlockPadding(constants.MessageAI)
	contentW := max(blockWidth-2*hPad, 1)

	rendered, err := aiMarkdownCache.renderMarkdown(contentW, text)
	if err != nil {
		return renderAIMessagePlain(blockWidth, text)
	}
	rendered = strings.TrimRight(rendered, "\n")
	if rendered == "" {
		return renderAIMessagePlain(blockWidth, text)
	}

	lineStyle := lipgloss.NewStyle().Padding(0, hPad).Width(blockWidth)
	lines := strings.Split(rendered, "\n")
	for i, line := range lines {
		lines[i] = lineStyle.Render(line)
	}
	return strings.Join(lines, "\n")
}

// renderAIMessage uses a cheap plain path while streaming or when Glamour is
// still running in the background. Full markdown formatting runs once after the
// response is complete (sync for small messages, async for large ones).
func renderAIMessage(blockWidth int, text string, streaming, glamourPending bool) string {
	if strings.TrimSpace(text) == "" {
		return ""
	}
	if streaming || glamourPending || !looksLikeMarkdown(text) {
		return renderAIMessagePlain(blockWidth, text)
	}
	return renderAIMessageGlamour(blockWidth, text)
}

func glamourRenderCmd(index, width int, source string) tea.Cmd {
	return func() tea.Msg {
		output := renderAIMessageGlamour(width, source)
		return glamourRenderMsg{
			index:  index,
			width:  width,
			source: source,
			output: output,
		}
	}
}

func (m Model) handleGlamourRenderMsg(msg glamourRenderMsg) (Model, tea.Cmd) {
	if msg.index < 0 || msg.index >= len(m.messages) {
		return m, nil
	}
	if m.messages[msg.index].text != msg.source {
		return m, nil
	}

	m.messages[msg.index].glamourPending = false
	m.messages[msg.index].renderCache = messageRenderCache{
		width:     msg.width,
		sourceLen: len(msg.source),
		streaming: false,
		output:    msg.output,
	}
	m.layout.ContentDirty = true
	m = m.syncLayout(m.content.AtBottom())
	return m, nil
}

func (m Model) scheduleGlamourRender(index int) (Model, tea.Cmd) {
	if index < 0 || index >= len(m.messages) {
		return m, nil
	}
	msg := m.messages[index]
	if msg.kind != constants.MessageAI || !looksLikeMarkdown(msg.text) || len(msg.text) < glamourAsyncMinLen {
		return m, nil
	}

	width := m.messageAreaWidth()
	m.messages[index].glamourPending = true
	m.messages[index].renderCache = messageRenderCache{}
	m.layout.ContentDirty = true
	return m, glamourRenderCmd(index, width, msg.text)
}
