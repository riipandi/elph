package renderer

import (
	"regexp"
	"strings"
	"sync"

	tea "charm.land/bubbletea/v2"
	"charm.land/glamour/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/theme"
)

// All markdown formatting runs off the UI thread so stream completion never
// blocks the event loop, regardless of response size.

// markdownLinkStripper is used by renderAIMessageGlamour to pre-process links
// before glamour rendering, preventing URL duplication in the output.
var markdownLinkStripper = regexp.MustCompile(`\[([^\]]*)\]\(([^)]*)\)`)

// OSC 8 hyperlink helpers: ESC ]8 ; ; URL ESC \ TEXT ESC ]8 ; ; ESC \
func writeHyperlink(b *strings.Builder, text, url string) {
	b.WriteString("\x1b]8;;")
	b.WriteString(url)
	b.WriteString("\x1b\\")
	b.WriteString(text)
	b.WriteString("\x1b]8;;")
	b.WriteString("\x1b\\")
}

func wrapHyperlink(text, url string) string {
	var b strings.Builder
	b.Grow(len(text) + len(url) + 20)
	writeHyperlink(&b, text, url)
	return b.String()
}

const glamourAsyncMinLen = 1

// stripMarkdownSyntax converts basic markdown formatting to clean plain text
// in a single pass (no regex, no per-pattern allocations).
// Used in the plain renderer path (glamour-pending) so users see readable
// text instead of raw markdown syntax.
func stripMarkdownSyntax(s string) string {
	// Strip ATX heading markers (### Title → Title).
	// Do this first so the inline scanner below doesn't need
	// line-start tracking complexity.
	if strings.Contains(s, "#") {
		s = stripATXHeadings(s)
	}

	// Fast path: scan for the first byte that could be markdown syntax.
	i := strings.IndexAny(s, "*_`[")
	if i < 0 {
		return s
	}
	var b strings.Builder
	b.Grow(len(s))
	// Copy the prefix before the first marker.
	b.WriteString(s[:i])
	for i < len(s) {
		switch s[i] {
		case '*':
			if i+1 < len(s) && s[i+1] == '*' {
				// **bold**
				j := strings.Index(s[i+2:], "**")
				if j >= 0 {
					b.WriteString(s[i+2 : i+2+j])
					i += 4 + j
					continue
				}
			}
			// *italic*
			j := strings.Index(s[i+1:], "*")
			if j >= 0 {
				b.WriteString(s[i+1 : i+1+j])
				i += 2 + j
				continue
			}
			b.WriteByte(s[i])
			i++

		case '_':
			if i+1 < len(s) && s[i+1] == '_' {
				// __bold__
				j := strings.Index(s[i+2:], "__")
				if j >= 0 {
					b.WriteString(s[i+2 : i+2+j])
					i += 4 + j
					continue
				}
			}
			// _italic_
			j := strings.Index(s[i+1:], "_")
			if j >= 0 {
				b.WriteString(s[i+1 : i+1+j])
				i += 2 + j
				continue
			}
			b.WriteByte(s[i])
			i++

		case '`':
			// `code`
			j := strings.Index(s[i+1:], "`")
			if j >= 0 {
				b.WriteString(s[i+1 : i+1+j])
				i += 2 + j
				continue
			}
			b.WriteByte(s[i])
			i++

		case '[':
			// [text](url) → text wrapped in OSC 8 hyperlink (clickable).
			// When text == url, just output the URL (no OSC 8 needed —
			// it's already visible as plain text).
			j := strings.Index(s[i:], "](")
			if j >= 0 {
				k := strings.Index(s[i+j+2:], ")")
				if k >= 0 {
					text := s[i+1 : i+j]
					url := s[i+j+2 : i+j+2+k]
					if text == url {
						b.WriteString(text)
					} else {
						writeHyperlink(&b, text, url)
					}
					i += j + 3 + k
					continue
				}
			}
			b.WriteByte(s[i])
			i++

		default:
			b.WriteByte(s[i])
			i++
		}
	}
	return b.String()
}

// stripATXHeadings removes ATX heading markers (up to 6 # followed by space)
// from the start of each line.
func stripATXHeadings(s string) string {
	lines := strings.Split(s, "\n")
	for i, line := range lines {
		// Count leading '#' characters
		j := 0
		for j < len(line) && line[j] == '#' {
			j++
		}
		// ATX heading: up to 6 # followed by a space
		if j > 0 && j <= 6 && j < len(line) && line[j] == ' ' {
			lines[i] = line[j+1:]
		}
	}
	return strings.Join(lines, "\n")
}

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
}

func glamourStylePath() string {
	if theme.IsDark() {
		return "dark"
	}
	return "light"
}

func resetMarkdownCache() {
	aiMarkdownCache.mu.Lock()
	defer aiMarkdownCache.mu.Unlock()
	aiMarkdownCache.renderer = nil
	aiMarkdownCache.width = 0
	aiMarkdownCache.style = ""
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
	return renderStyledMessage(blockWidth, constants.MessageAI, stripMarkdownSyntax(text))
}

func renderAIMessageGlamour(blockWidth int, text string) string {
	_, hPad := messageBlockPadding(constants.MessageAI)
	contentW := max(blockWidth-2*hPad, 1)

	// Strip markdown link syntax before glamour rendering:
	// [text](url) → text wrapped in OSC 8 hyperlink (text is visible,
	// url is embedded as clickable). When text == url, just output the
	// URL (no wrapping needed — it's already visible).
	processed := markdownLinkStripper.ReplaceAllStringFunc(text, func(match string) string {
		parts := markdownLinkStripper.FindStringSubmatch(match)
		if len(parts) != 3 {
			return match
		}
		if parts[1] == parts[2] {
			return parts[1]
		}
		return wrapHyperlink(parts[1], parts[2])
	})

	rendered, err := aiMarkdownCache.renderMarkdown(contentW, processed)
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
