package renderer

import (
	"regexp"
	"strings"
	"sync"
	"unicode"
	"unicode/utf8"

	tea "charm.land/bubbletea/v2"
	"charm.land/glamour/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/theme"
	"github.com/riipandi/elph/pkg/core/agent"
)

// Markdown rendering:
//   - complete messages with markup → Glamour terminal renderer
//   - streaming / async pending → plain or source preview
//   - plain prose → formatAIProse

const markdownAsyncMinLen = 1

var (
	markdownLinkStripper  = regexp.MustCompile(`\[([^\]]*)\]\(([^)]*)\)`)
	markdownImageStripper = regexp.MustCompile(`!\[([^\]]*)\]\(([^)]*)\)`)
	footnoteDefLine       = regexp.MustCompile(`(?m)^\[\^([^\]]+)\]:\s*(.+)\s*$`)
	footnoteRef           = regexp.MustCompile(`\[\^([^\]]+)\]`)
	abbrDefLine           = regexp.MustCompile(`(?m)^\*\[([^\]]+)\]:\s*.+\s*$`)
	htmlDetailsBlock      = regexp.MustCompile(`(?is)<details>\s*<summary>([^<]*)</summary>\s*([\s\S]*?)</details>`)
	htmlTagStripper       = regexp.MustCompile(`</?[a-zA-Z][^>]*>`)
	extraBlankLines       = regexp.MustCompile(`\n{3,}`)
)

func wrapHyperlink(text, url string) string {
	return ansi.SetHyperlink(url) + text + ansi.ResetHyperlink()
}

func collapseDuplicateMarkdownLinks(text string) string {
	return markdownLinkStripper.ReplaceAllStringFunc(text, func(match string) string {
		parts := markdownLinkStripper.FindStringSubmatch(match)
		if len(parts) == 3 && parts[1] == parts[2] {
			return parts[1]
		}
		return match
	})
}

type markdownRenderCache struct {
	mu       sync.Mutex
	width    int
	style    string
	renderer *glamour.TermRenderer
}

var aiMarkdownCache markdownRenderCache

type markdownRenderMsg struct {
	index  int
	width  int
	source string
	output string
}

func glamourStyleKey() string {
	if theme.IsDark() {
		return "dark"
	}
	return "light"
}

func blockquoteDepth(line string) (depth int, ok bool) {
	trimmed := strings.TrimLeft(line, " \t")
	if !strings.HasPrefix(trimmed, ">") {
		return 0, false
	}
	i := 0
	for i < len(trimmed) {
		for i < len(trimmed) && trimmed[i] == ' ' {
			i++
		}
		if i >= len(trimmed) || trimmed[i] != '>' {
			break
		}
		depth++
		i++
	}
	return depth, depth > 0
}

// normalizeBlockquoteMarkdown inserts empty ">" lines when quote depth decreases so
// goldmark closes nested blockquotes before shallower lines (e.g. "> > nested" then "> outer").
func normalizeBlockquoteMarkdown(text string) string {
	lines := strings.Split(text, "\n")
	out := make([]string, 0, len(lines)+4)
	prevDepth := 0
	for _, line := range lines {
		depth, isQuote := blockquoteDepth(line)
		if isQuote && prevDepth > 0 && depth > 0 && depth < prevDepth {
			for range prevDepth - depth {
				out = append(out, ">")
			}
		}
		out = append(out, line)
		switch {
		case isQuote:
			prevDepth = depth
		case strings.TrimSpace(line) == "":
			// Blank lines do not end an open blockquote.
		default:
			prevDepth = 0
		}
	}
	return strings.Join(out, "\n")
}

func collapseExtraBlankLines(text string) string {
	return extraBlankLines.ReplaceAllString(strings.TrimSpace(text), "\n\n")
}

func preprocessFootnotes(text string) string {
	defs := make(map[string]string)
	var order []string
	text = footnoteDefLine.ReplaceAllStringFunc(text, func(match string) string {
		parts := footnoteDefLine.FindStringSubmatch(match)
		if len(parts) != 3 {
			return match
		}
		label := parts[1]
		if _, ok := defs[label]; !ok {
			order = append(order, label)
		}
		defs[label] = strings.TrimSpace(parts[2])
		return ""
	})
	if len(defs) == 0 {
		return text
	}
	text = collapseExtraBlankLines(text)
	text = footnoteRef.ReplaceAllStringFunc(text, func(match string) string {
		parts := footnoteRef.FindStringSubmatch(match)
		if len(parts) != 2 {
			return match
		}
		return "(" + parts[1] + ")"
	})
	var notes strings.Builder
	notes.WriteString("\n\n")
	for _, label := range order {
		notes.WriteString("> [")
		notes.WriteString(label)
		notes.WriteString("] ")
		notes.WriteString(defs[label])
		notes.WriteString("\n")
	}
	return strings.TrimRight(text, "\n") + notes.String()
}

func preprocessHTMLBlocks(text string) string {
	text = htmlDetailsBlock.ReplaceAllStringFunc(text, func(match string) string {
		parts := htmlDetailsBlock.FindStringSubmatch(match)
		if len(parts) != 3 {
			return match
		}
		summary := strings.TrimSpace(parts[1])
		body := strings.TrimSpace(htmlTagStripper.ReplaceAllString(parts[2], ""))
		if body == "" {
			return "> **" + summary + "**"
		}
		return "> **" + summary + "**\n\n" + body
	})
	return htmlTagStripper.ReplaceAllString(text, "")
}

func stripUnsupportedMarkdownDefs(text string) string {
	text = abbrDefLine.ReplaceAllString(text, "")
	return collapseExtraBlankLines(text)
}

func stripAIProseSeparatorLines(text string) string {
	lines := strings.Split(text, "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		if isAIProseSeparatorLine(line) {
			continue
		}
		out = append(out, line)
	}
	return strings.Join(out, "\n")
}

func preprocessMarkdownForGlamour(text string) string {
	text = normalizeBlockquoteMarkdown(text)
	text = preprocessHTMLBlocks(text)
	text = preprocessFootnotes(text)
	text = stripUnsupportedMarkdownDefs(text)
	// Images before links: ![alt](url) also matches the link pattern on [alt](url).
	text = markdownImageStripper.ReplaceAllStringFunc(text, func(match string) string {
		parts := markdownImageStripper.FindStringSubmatch(match)
		if len(parts) != 3 {
			return match
		}
		if alt := strings.TrimSpace(parts[1]); alt != "" {
			return alt
		}
		return "image"
	})
	text = collapseDuplicateMarkdownLinks(text)
	text = markdownLinkStripper.ReplaceAllStringFunc(text, func(match string) string {
		parts := markdownLinkStripper.FindStringSubmatch(match)
		if len(parts) != 3 {
			return match
		}
		if parts[1] == parts[2] {
			return parts[1]
		}
		return wrapHyperlink(parts[1], parts[2])
	})
	return text
}

func resetMarkdownCache() {
	aiMarkdownCache.mu.Lock()
	defer aiMarkdownCache.mu.Unlock()
	aiMarkdownCache.renderer = nil
	aiMarkdownCache.width = 0
	aiMarkdownCache.style = ""
}

func (c *markdownRenderCache) renderMarkdown(width int, markdown string) (string, error) {
	styleKey := glamourStyleKey()

	c.mu.Lock()
	defer c.mu.Unlock()

	if c.renderer == nil || c.width != width || c.style != styleKey {
		r, err := glamour.NewTermRenderer(
			glamour.WithStyles(elphGlamourStyleConfig()),
			glamour.WithWordWrap(width),
			glamour.WithPreservedNewLines(),
			glamour.WithTableWrap(false),
			glamour.WithEmoji(),
		)
		if err != nil {
			c.renderer = nil
			return "", err
		}
		c.renderer = r
		c.width = width
		c.style = styleKey
	}

	return c.renderer.Render(markdown)
}

func looksLikeTableLine(trimmed string) bool {
	if !strings.Contains(trimmed, "|") {
		return false
	}
	if strings.HasPrefix(trimmed, "|") {
		return true
	}
	return strings.Count(trimmed, "|") >= 2
}

func isMarkdownBlockLine(line string, inFence bool) bool {
	if inFence {
		return true
	}
	trimmed := strings.TrimSpace(line)
	if strings.HasPrefix(trimmed, "```") {
		return true
	}
	switch {
	case strings.HasPrefix(trimmed, "#"),
		strings.HasPrefix(trimmed, ">"),
		looksLikeTableLine(trimmed),
		isHorizontalRuleLine(trimmed),
		strings.HasPrefix(trimmed, "- "),
		strings.HasPrefix(trimmed, "* "),
		strings.HasPrefix(trimmed, "+ "):
		return true
	}
	return len(trimmed) > 2 && trimmed[0] >= '0' && trimmed[0] <= '9' && strings.Contains(trimmed, ". ")
}

func hasMarkdownBlockStructure(text string) bool {
	inFence := false
	for _, line := range strings.Split(text, "\n") {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "```") {
			inFence = !inFence
			return true
		}
		if isMarkdownBlockLine(line, inFence) {
			return true
		}
	}
	return false
}

func takeMarkdownLink(s string, i int) (text, url string, next int, ok bool) {
	if i >= len(s) || s[i] != '[' {
		return "", "", i, false
	}
	j := strings.Index(s[i:], "](")
	if j < 0 {
		return "", "", i, false
	}
	k := strings.Index(s[i+j+2:], ")")
	if k < 0 {
		return "", "", i, false
	}
	return s[i+1 : i+j], s[i+j+2 : i+j+2+k], i + j + 3 + k, true
}

func takeMarkdownImageLink(s string, i int) (alt, url string, next int, ok bool) {
	if i >= len(s) || !strings.HasPrefix(s[i:], "![") {
		return "", "", i, false
	}
	j := strings.Index(s[i+2:], "](")
	if j < 0 {
		return "", "", i, false
	}
	k := strings.Index(s[i+j+4:], ")")
	if k < 0 {
		return "", "", i, false
	}
	return s[i+2 : i+2+j], s[i+j+4 : i+j+4+k], i + j + 5 + k, true
}

func stripATXHeadings(s string) string {
	lines := strings.Split(s, "\n")
	for i, line := range lines {
		j := 0
		for j < len(line) && line[j] == '#' {
			j++
		}
		if j > 0 && j <= 6 && j < len(line) && line[j] == ' ' {
			lines[i] = line[j+1:]
		}
	}
	return strings.Join(lines, "\n")
}

// stripMarkdownSyntax converts inline markdown to plain text for the non-markdown path.
func stripMarkdownSyntax(s string) string {
	if strings.Contains(s, "#") {
		s = stripATXHeadings(s)
	}
	i := strings.IndexAny(s, "*_`[!")
	if i < 0 {
		return s
	}
	var b strings.Builder
	b.Grow(len(s))
	b.WriteString(s[:i])
	inFence := false
	for i < len(s) {
		if strings.HasPrefix(s[i:], "```") {
			b.WriteString("```")
			i += 3
			inFence = !inFence
			continue
		}
		if inFence {
			b.WriteByte(s[i])
			i++
			continue
		}
		switch s[i] {
		case '*':
			if i+1 < len(s) && s[i+1] == '*' {
				j := strings.Index(s[i+2:], "**")
				if j >= 0 {
					b.WriteString(s[i+2 : i+2+j])
					i += 4 + j
					continue
				}
			}
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
				j := strings.Index(s[i+2:], "__")
				if j >= 0 {
					b.WriteString(s[i+2 : i+2+j])
					i += 4 + j
					continue
				}
			}
			j := strings.Index(s[i+1:], "_")
			if j >= 0 {
				b.WriteString(s[i+1 : i+1+j])
				i += 2 + j
				continue
			}
			b.WriteByte(s[i])
			i++
		case '`':
			j := strings.Index(s[i+1:], "`")
			if j >= 0 {
				b.WriteString(s[i+1 : i+1+j])
				i += 2 + j
				continue
			}
			b.WriteByte(s[i])
			i++
		case '!':
			if i+1 < len(s) && s[i+1] == '[' {
				if alt, _, ni, ok := takeMarkdownImageLink(s, i); ok {
					if alt == "" {
						alt = "image"
					}
					b.WriteString(alt)
					i = ni
					continue
				}
			}
			b.WriteByte(s[i])
			i++
		case '[':
			if text, _, ni, ok := takeMarkdownLink(s, i); ok {
				b.WriteString(text)
				i = ni
				continue
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

func hasInlineAsteriskEmphasis(s string) bool {
	if !strings.Contains(s, "*") {
		return false
	}
	for i := 0; i < len(s); i++ {
		if s[i] != '*' {
			continue
		}
		if i+1 < len(s) && s[i+1] == '*' {
			continue
		}
		if i+1 < len(s) && s[i+1] == ' ' {
			continue
		}
		j := strings.Index(s[i+1:], "*")
		if j <= 0 {
			continue
		}
		if inner := strings.TrimSpace(s[i+1 : i+1+j]); inner != "" {
			return true
		}
	}
	return false
}

func hasInlineUnderscoreEmphasis(s string) bool {
	if !strings.Contains(s, "_") {
		return false
	}
	for i := 0; i < len(s); i++ {
		if s[i] != '_' {
			continue
		}
		if i+1 < len(s) && s[i+1] == '_' {
			continue
		}
		j := strings.Index(s[i+1:], "_")
		if j <= 0 {
			continue
		}
		if inner := strings.TrimSpace(s[i+1 : i+1+j]); inner != "" {
			return true
		}
	}
	return false
}

func looksLikeMarkdown(text string) bool {
	if hasMarkdownBlockStructure(text) {
		return true
	}
	for _, line := range strings.Split(text, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}
		if strings.Contains(trimmed, "**") ||
			strings.Contains(trimmed, "__") ||
			strings.Contains(trimmed, "~~") ||
			strings.Contains(trimmed, "`") ||
			strings.Contains(trimmed, "![") ||
			strings.Contains(trimmed, "](") ||
			hasInlineAsteriskEmphasis(trimmed) ||
			hasInlineUnderscoreEmphasis(trimmed) {
			return true
		}
	}
	return false
}

func isHorizontalRuleLine(line string) bool {
	line = strings.TrimSpace(line)
	if len(line) < 3 {
		return false
	}
	r := line[0]
	if r != '-' && r != '*' && r != '_' {
		return false
	}
	for i := 1; i < len(line); i++ {
		if line[i] != r {
			return false
		}
	}
	return true
}

func isAIProseSeparatorLine(line string) bool {
	line = strings.TrimSpace(line)
	if len(line) < 3 {
		return false
	}
	r := line[0]
	if r != '-' && r != '*' && r != '_' && r != '=' {
		return false
	}
	for i := 1; i < len(line); i++ {
		if line[i] != r {
			return false
		}
	}
	return true
}

func normalizeAIProseSeparators(text string) string {
	lines := strings.Split(text, "\n")
	if len(lines) == 0 {
		return text
	}
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		if isAIProseSeparatorLine(line) {
			if len(out) > 0 && out[len(out)-1] != "" {
				out = append(out, "")
			}
			continue
		}
		out = append(out, line)
	}
	return strings.Join(out, "\n")
}

func endsAISentence(s string) bool {
	s = strings.TrimSpace(s)
	s = strings.TrimRight(s, "\"'”»")
	if s == "" {
		return false
	}
	r, _ := utf8.DecodeLastRuneInString(s)
	return r == '.' || r == '!' || r == '?'
}

func startsAIParagraphLine(line string) bool {
	line = strings.TrimLeft(line, " \t")
	if line == "" {
		return false
	}
	r, _ := utf8.DecodeRuneInString(line)
	return unicode.IsUpper(r) || unicode.IsDigit(r)
}

func shouldAIProseParagraphBreak(prev, next string, width int) bool {
	next = strings.TrimLeft(next, " \t")
	if next == "" {
		return false
	}
	if r, _ := utf8.DecodeRuneInString(next); unicode.IsLower(r) {
		return false
	}
	prev = strings.TrimSpace(prev)
	if strings.HasSuffix(prev, "-") {
		return false
	}
	if strings.HasSuffix(prev, ",") || strings.HasSuffix(prev, ";") || strings.HasSuffix(prev, ":") {
		return false
	}
	if !endsAISentence(prev) {
		return false
	}
	if width > 0 && lipgloss.Width(prev) >= width-2 {
		return false
	}
	if startsAIParagraphLine(next) {
		return true
	}
	return width > 0 &&
		lipgloss.Width(prev) < width*3/5 &&
		lipgloss.Width(next) < width*3/5
}

func joinAIProseLines(prev, next string) string {
	prev = strings.TrimSpace(prev)
	next = strings.TrimSpace(next)
	if prev == "" {
		return next
	}
	if next == "" {
		return prev
	}
	if strings.HasSuffix(prev, "-") {
		if r, _ := utf8.DecodeRuneInString(next); unicode.IsLower(r) {
			return strings.TrimSuffix(prev, "-") + next
		}
	}
	if r, _ := utf8.DecodeRuneInString(next); r != utf8.RuneError {
		switch r {
		case '.', ',', ';', ':', '!', '?', ')', ']', '%', '\'', '"':
			return prev + next
		}
	}
	return prev + " " + next
}

func splitAIProseParagraphs(text string, width int) []string {
	var paras []string
	for _, chunk := range strings.Split(text, "\n\n") {
		chunk = strings.TrimSpace(chunk)
		if chunk == "" {
			continue
		}
		var current strings.Builder
		flush := func() {
			if s := strings.TrimSpace(current.String()); s != "" {
				paras = append(paras, s)
			}
			current.Reset()
		}
		for _, line := range strings.Split(chunk, "\n") {
			line = strings.TrimSpace(line)
			if line == "" || isAIProseSeparatorLine(line) {
				flush()
				continue
			}
			if current.Len() > 0 {
				prev := current.String()
				if shouldAIProseParagraphBreak(prev, line, width) {
					flush()
					current.WriteString(line)
				} else {
					current.Reset()
					current.WriteString(joinAIProseLines(prev, line))
				}
				continue
			}
			current.WriteString(line)
		}
		flush()
	}
	return paras
}

func wrapAIProseParagraph(para string, width int) string {
	para = strings.Join(strings.Fields(para), " ")
	if para == "" {
		return ""
	}
	if width < 1 {
		return para
	}
	return ansi.Hardwrap(ansi.Wordwrap(para, width, ""), width, false)
}

func formatAIProse(text string, width int) string {
	paras := splitAIProseParagraphs(text, width)
	if len(paras) == 0 {
		return ""
	}
	out := make([]string, 0, len(paras))
	for _, para := range paras {
		if wrapped := wrapAIProseParagraph(para, width); wrapped != "" {
			out = append(out, wrapped)
		}
	}
	return strings.Join(out, "\n\n")
}

func wrapAILine(line string, width int) string {
	if line == "" || width < 1 {
		return line
	}
	return ansi.Hardwrap(ansi.Wordwrap(line, width, ""), width, false)
}

// renderAISourcePreview soft-wraps each source line without parsing markdown.
func renderAISourcePreview(blockWidth int, text string) string {
	width := aiContentWidth(blockWidth)
	lines := strings.Split(text, "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		if strings.TrimSpace(line) == "" {
			out = append(out, "")
			continue
		}
		out = append(out, wrapAILine(line, width))
	}
	return renderAIBlock(blockWidth, strings.Join(out, "\n"), false)
}

func renderAIMessagePlain(blockWidth int, text string, streaming bool) string {
	if !streaming {
		text = stripMarkdownSyntax(text)
	}
	body := formatAIProse(text, aiContentWidth(blockWidth))
	return renderAIBlock(blockWidth, body, false)
}

func renderAIMarkdown(blockWidth int, text string) string {
	contentW := aiContentWidth(blockWidth)
	processed := preprocessMarkdownForGlamour(text)

	rendered, err := aiMarkdownCache.renderMarkdown(contentW, processed)
	if err != nil {
		return renderAIMessagePlain(blockWidth, text, false)
	}
	rendered = strings.TrimRight(rendered, "\n")
	if rendered == "" {
		return renderAIMessagePlain(blockWidth, text, false)
	}
	return renderAIPreformattedBlock(blockWidth, rendered, false)
}

func renderAIMessage(blockWidth int, text string, streaming, markdownPending bool) string {
	text = agent.SanitizeAssistantDisplay(text)
	if strings.TrimSpace(text) == "" {
		return ""
	}
	switch {
	case markdownPending:
		return renderAISourcePreview(blockWidth, text)
	case streaming || !looksLikeMarkdown(text):
		if !streaming && !looksLikeMarkdown(text) {
			text = normalizeAIProseSeparators(text)
		}
		return renderAIMessagePlain(blockWidth, text, streaming)
	default:
		return renderAIMarkdown(blockWidth, text)
	}
}

func markdownRenderCmd(index, width int, source string) tea.Cmd {
	return func() tea.Msg {
		return markdownRenderMsg{
			index:  index,
			width:  width,
			source: source,
			output: renderAIMarkdown(width, source),
		}
	}
}

func (m Model) handleMarkdownRenderMsg(msg markdownRenderMsg) (Model, tea.Cmd) {
	if msg.index < 0 || msg.index >= len(m.messages) {
		return m, nil
	}
	if m.messages[msg.index].text != msg.source {
		return m, nil
	}
	m.messages[msg.index].markdownPending = false
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

func (m Model) scheduleMarkdownRender(index int) (Model, tea.Cmd) {
	if index < 0 || index >= len(m.messages) {
		return m, nil
	}
	msg := m.messages[index]
	if msg.kind != constants.MessageAI || !looksLikeMarkdown(msg.text) || len(msg.text) < markdownAsyncMinLen {
		return m, nil
	}
	width := m.messageAreaWidth()
	m.messages[index].markdownPending = true
	m.layout.ContentDirty = true
	return m, markdownRenderCmd(index, width, msg.text)
}
