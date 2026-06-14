package renderer

import (
	"fmt"
	"image/color"
	"path/filepath"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/config"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
)

// ─── Cached Styles ────────────────────────────────────────────────────────────
// Package-level cached styles to avoid per-frame allocation.
var (
	cachedBannerBorder = lipgloss.NewStyle().
				Border(lipgloss.RoundedBorder()).
				BorderForeground(constants.Blue).
				Padding(1, 2)

	dimStyle     = lipgloss.NewStyle().Foreground(constants.DimText)
	valStyle     = lipgloss.NewStyle().Foreground(constants.BrightText)
	whiteSty     = lipgloss.NewStyle().Foreground(constants.White)
	whiteBoldSty = lipgloss.NewStyle().Foreground(constants.White).Bold(true)
	sidSty       = lipgloss.NewStyle().Foreground(constants.DimText)
	yellowSty    = lipgloss.NewStyle().Foreground(constants.Yellow).Italic(true)
	metaSty      = lipgloss.NewStyle().Foreground(constants.DimText)
)

// ─── View ────────────────────────────────────────────────────────────────────

func (m Model) View() tea.View {
	if m.quitting {
		return tea.NewView("")
	}
	if !m.ready {
		return tea.NewView("\n  Initializing...")
	}

	v := tea.NewView(lipgloss.JoinVertical(lipgloss.Top, m.viewParts()...))
	v.AltScreen = true
	if m.mouseEnabled && !m.selectingText {
		v.MouseMode = tea.MouseModeCellMotion
	}
	return v
}

// viewParts returns the stacked UI layers below the scrollable viewport.
// Empty layers are omitted so JoinVertical does not insert blank lines.
func (m Model) viewParts() []string {
	parts := []string{m.contentAreaView()}
	if av := m.activityView(); av != "" {
		parts = append(parts, av)
	}
	parts = append(parts, m.inputChromeView(), m.footerView())
	return parts
}

func (m Model) renderedViewHeight() int {
	return lipgloss.Height(lipgloss.JoinVertical(lipgloss.Top, m.viewParts()...))
}

func (m Model) chromeHeight() int {
	h := lipgloss.Height(m.inputChromeView()) + lipgloss.Height(m.footerView())
	if m.showsActivity() {
		h += lipgloss.Height(m.activityView())
	}
	return h
}

// syncLayout sizes chrome and viewport. Rebuilds scrollable content only when
// dirty. When follow is true the viewport scrolls to the newest lines.
func (m Model) syncLayout(follow bool) Model {
	if !m.ready || m.width <= 0 || m.height <= 0 {
		return m
	}

	m = m.syncInputWidth()

	atBottom := m.content.AtBottom()

	prevH := m.content.Height()
	m.content.SetHeight(max(m.height-m.chromeHeight(), 1))

	// Build content at guttered width immediately so message backgrounds never
	// span the scrollbar column. Always reserve the gutter (targetContentWidth)
	// so width remains stable — no cache-miss chain on scrollable transition.
	prevContentW := m.content.Width()
	targetW := m.targetContentWidth()
	m.content.SetWidth(targetW)
	needsRebuild := m.layout.ContentDirty || prevH != m.content.Height() || prevContentW != targetW
	if needsRebuild {
		if m.streamingMessageIndex() >= 0 {
			m = m.refreshStreamPrefixCache()
		} else {
			m = m.clearStreamPrefixCache()
		}
		m.content.SetContent(m.contentView())
		m.layout.ContentDirty = false
	}

	// Bubble Tea drops lines from the top when output exceeds terminal height,
	// which clips the banner border. Shrink the viewport until the frame fits.
	for m.renderedViewHeight() > m.height && m.content.Height() > 1 {
		m.content.SetHeight(m.content.Height() - 1)
	}

	m.layout.ChromeH = m.chromeHeight()
	m = m.syncInputWidth()

	if follow || atBottom {
		m.content.GotoBottom()
	} else if len(m.messages) == 0 {
		m.content.GotoTop()
	}

	return m
}

func (m Model) syncInputWidth() Model {
	prefixW := 0
	if m.showPromptPrefix {
		prefix := lipgloss.NewStyle().Foreground(constants.White).Bold(true).Render(m.promptChar + " ")
		prefixW = lipgloss.Width(prefix)
	}

	inputW := inputContentWidth(m.chromeOuterWidth()) - prefixW
	if inputW < 1 {
		inputW = 1
	}
	m.layout.InputWidth = inputW
	m.input.SetWidth(inputW)
	m = m.syncInputHeight()
	return m.syncInputScroll()
}

const messageBlockGap = "\n\n"

// contentView is the full scrollable region: banner + message history.
func (m Model) contentView() string {
	var b strings.Builder

	b.WriteString(m.bannerView())
	if len(m.messages) > 0 {
		b.WriteString(messageBlockGap)
		b.WriteString(m.messagesView())
	}

	return b.String()
}

// messagesView renders chat history with one blank line between every block.
// While a message is streaming, previously rendered prefix blocks are reused.
func (m Model) messagesView() string {
	n := len(m.messages)
	if n == 0 {
		return ""
	}

	streamIdx := m.streamingMessageIndex()
	if streamIdx >= 0 &&
		m.layout.StreamPrefixUpTo == streamIdx &&
		streamIdx < n {
		var b strings.Builder
		b.WriteString(m.layout.StreamPrefix)
		if streamIdx > 0 {
			b.WriteString(messageBlockGap)
		}
		b.WriteString(m.renderMessageAt(streamIdx))
		for i := streamIdx + 1; i < n; i++ {
			b.WriteString(messageBlockGap)
			b.WriteString(m.renderMessageAt(i))
		}
		return b.String()
	}

	var b strings.Builder
	for i := range m.messages {
		if i > 0 {
			b.WriteString(messageBlockGap)
		}
		b.WriteString(m.renderMessageAt(i))
	}
	return b.String()
}

func (m Model) renderMessage(msg message) string {
	width := m.messageAreaWidth()
	if msg.kind == constants.MessageAI {
		return renderAIMessage(width, msg.text, false, false)
	}
	return renderStyledMessage(width, msg.kind, msg.text)
}

func (m Model) renderMessageAt(index int) string {
	msg := m.messages[index]
	width := m.messageAreaWidth()
	streaming := m.isStreamingMessageAt(index)

	if c := msg.renderCache; c.hit(width, streaming, len(msg.text)) {
		return c.output
	}

	var out string
	switch {
	case streaming:
		out = renderStreamingMessage(width, msg.kind, msg.text)
	case msg.kind == constants.MessageAI:
		out = renderAIMessage(width, msg.text, false, msg.glamourPending)
	default:
		out = renderStyledMessage(width, msg.kind, msg.text)
	}

	m.messages[index].renderCache = messageRenderCache{
		width:     width,
		sourceLen: len(msg.text),
		streaming: streaming,
		output:    out,
	}
	return out
}

func messageBlockPadding(kind constants.MessageKind) (vertical, horizontal int) {
	switch kind {
	case constants.MessageUser, constants.MessageTool:
		return 1, 2
	default:
		return 0, 1
	}
}

// renderStyledMessage paints each message block. Vertical spacing between blocks
// comes from messageBlockGap; boxed kinds also get internal vertical padding.
func renderStyledMessage(width int, kind constants.MessageKind, text string) string {
	vPad, hPad := messageBlockPadding(kind)
	// Use a single Render call — Width() already applies per-line padding,
	// so the per-line loop below is unnecessary and creates O(n) string
	// allocations that add GC pressure on every content rebuild.
	return constants.MessageStyle(kind).
		Padding(vPad, hPad).
		Width(width).
		Render(text)
}

// bannerContentWidth is the usable text width inside the banner border and padding.
func bannerContentWidth(terminalW int) int {
	return max(terminalW-6, 10)
}

// footerContentWidth is the usable text width for footer rows (1-char left padding).
func footerContentWidth(terminalW int) int {
	return max(terminalW-2, 1)
}

// clampLine truncates styled content to a single line (line-clamp).
func clampLine(maxW int, s string) string {
	if maxW <= 0 {
		return ""
	}
	return lipgloss.NewStyle().MaxWidth(maxW).Inline(true).Render(s)
}

// metaLine renders a dim label + bright value, truncated as one line.
func metaLine(maxW int, label, value string) string {
	return clampLine(maxW, dimStyle.Render(label)+valStyle.Render(value))
}

// wrapLine word-wraps styled content within the given width.
func wrapLine(width int, s string) string {
	if width <= 0 {
		return s
	}
	return lipgloss.NewStyle().Width(width).Inline(true).Render(s)
}

// footerRow renders a status line with a truncated left segment and a right segment
// flush to the right edge. The left block is width-fixed so JoinHorizontal pads
// the gap between the two sides.
func footerRow(contentW int, left, right string) string {
	rightW := lipgloss.Width(right)
	if rightW >= contentW {
		return clampLine(contentW, right)
	}
	leftW := max(contentW-rightW, 0)
	leftPart := lipgloss.NewStyle().Width(leftW).MaxHeight(1).Render(left)
	row := lipgloss.JoinHorizontal(lipgloss.Top, leftPart, right)
	return lipgloss.NewStyle().Width(contentW).MaxHeight(1).Render(row)
}

// ─── Sub-views ───────────────────────────────────────────────────────────────

func (m Model) bannerView() string {
	w := m.chromeOuterWidth()
	innerW := bannerContentWidth(w)

	// TODO: replace with actual value
	updateAvailable := false

	versionLine := fmt.Sprintf("Welcome to %s v%s", config.AppName, config.AppVersion)
	if updateAvailable {
		updateNotice := lipgloss.NewStyle().Foreground(constants.Yellow).Italic(true).Bold(false).Render("(update available)")
		versionLine = fmt.Sprintf("Welcome to %s v%s %s", config.AppName, config.AppVersion, updateNotice)
	}

	logo := lipgloss.JoinVertical(lipgloss.Left,
		lipgloss.NewStyle().Foreground(constants.GreenLt).Render(constants.LogoLine1),
		lipgloss.NewStyle().Foreground(constants.GreenLt).Render(constants.LogoLine2),
	)
	logoBlock := lipgloss.NewStyle().MarginRight(2).Render(logo)
	topW := max(innerW-lipgloss.Width(logoBlock), 10)

	header := clampLine(topW, lipgloss.NewStyle().Bold(true).Render(versionLine))
	subtitle := clampLine(topW, dimStyle.Render("Send /changelog to show version history."))

	topSection := lipgloss.JoinHorizontal(lipgloss.Top, logoBlock, lipgloss.JoinVertical(lipgloss.Left, header, subtitle))

	meta := lipgloss.JoinVertical(lipgloss.Left,
		"",
		metaLine(innerW, "Directory:  ", m.workDir),
		metaLine(innerW, "Model:      ", fmt.Sprintf("%s [%s] (%d available)", m.modelName, m.provider, m.availableModelCount())),
		metaLine(innerW, "Stats:      ", fmt.Sprintf("%d exts, %d commands, %d skills, %d tools", 0, 0, 0, 0)),
		metaLine(innerW, "MCP Server: ", fmt.Sprintf("%d/%d connected (%d tools)", 0, 0, 0)),
	)

	tipBody := dimStyle.Italic(true).Render(" " + m.tip)
	tip := wrapLine(innerW, yellowSty.Render("Tip:")+tipBody)

	content := lipgloss.JoinVertical(lipgloss.Left, topSection, meta, "", tip)

	return cachedBannerBorder.Width(borderedChromeWidth(w)).Render(content)
}

func (m Model) activityView() string {
	if !m.showsActivity() {
		return ""
	}
	frame := spinnerFrames[m.agent.SpinnerFrame%len(spinnerFrames)]
	spinner := lipgloss.NewStyle().Foreground(constants.Yellow).Render(frame)
	label := dimStyle.Render(" " + m.activityLabel() + "...")
	return lipgloss.NewStyle().
		MarginTop(1).
		PaddingLeft(1).
		Width(m.width).
		Render(spinner + label)
}

func (m Model) activityLabel() string {
	if m.shell.Running {
		cmd := m.shell.Command
		if cmd == "" {
			return string(agent.ActivityRunning)
		}
		prefix := "Running $ "
		suffix := " · Esc to cancel"
		maxCmd := m.width - lipgloss.Width(prefix+suffix+"...") - 2
		if maxCmd < 8 {
			maxCmd = 8
		}
		if lipgloss.Width(cmd) > maxCmd {
			cmd = clampLine(maxCmd, cmd)
		}
		return prefix + cmd + suffix
	}
	if m.agent.Busy {
		label := string(m.agent.Activity)
		if m.agent.Activity == agent.ActivityIdle {
			label = string(agent.ActivityWorking)
		}
		return label + " · Esc to cancel"
	}
	if m.agent.Activity == agent.ActivityIdle {
		return string(agent.ActivityWorking)
	}
	return string(m.agent.Activity)
}

func (m Model) inputBodyView() string {
	body := m.input.View()
	if !m.inputScrollable() {
		return body
	}
	// Draw the scrollbar in the last column of each line so we do not reserve a
	// separate gutter column with visible trailing padding before the bar.
	return overlayInputScrollBar(body, m.inputScrollBarView(), m.layout.InputWidth)
}

func (m Model) inputChromeView() string {
	if m.modelSelectorActive() {
		return m.modelSelectorChromeView()
	}

	palette := m.commandPaletteView()
	inputBox := m.inputBoxView(palette != "")

	if palette != "" {
		return lipgloss.JoinVertical(lipgloss.Top, palette, inputBox)
	}
	if !m.showsActivity() {
		return lipgloss.NewStyle().MarginTop(1).Render(inputBox)
	}
	return inputBox
}

func (m Model) inputBoxView(attached bool) string {
	border := cachedInputBorder(m.mode)
	if attached {
		border = cachedInputBorderAttached(m.mode)
	}
	boxW := borderedChromeWidth(m.chromeOuterWidth())
	inner := m.inputBodyView()
	if m.showPromptPrefix {
		prefix := lipgloss.NewStyle().Foreground(constants.White).Bold(true).Render(m.promptChar + " ")
		inner = prefix + inner
	}
	return border.Width(boxW).Render(inner)
}

func (m Model) inputView() string {
	return m.inputChromeView()
}

func (m Model) footerView() string {
	wd := filepath.Base(m.workDir)
	sidVal := m.sessionID.Suffix()

	cw := footerContentWidth(m.width)

	modelSty := lipgloss.NewStyle().Foreground(constants.ThinkingColor(m.thinkingLevel))
	line1Left := modelSty.Render(m.modelName) + metaSty.Render(fmt.Sprintf(" | %s | T: %s | IMG", m.provider, m.thinkingLevel))

	ctxColor := constants.ContextUsageColor(m.contextUsed)
	ctxSty := lipgloss.NewStyle().Foreground(ctxColor)
	line1Right := ctxSty.Render(fmt.Sprintf("$0.00 | %.1f%% (%s)", m.contextUsed*100, m.contextWindowLabel()))

	modeSty := lipgloss.NewStyle().Foreground(constants.ModeBorderColor(m.mode)).Bold(true)
	line2Left := whiteBoldSty.Render(wd) + sidSty.Render(fmt.Sprintf(" [%s] ", sidVal)) + modeSty.Render(string(m.mode))

	gitStr := "[-]"
	if m.gitAdded > 0 || m.gitDeleted > 0 {
		gitStr = fmt.Sprintf("[+%d -%d]", m.gitAdded, m.gitDeleted)
	}
	var gitColor color.Color
	switch {
	case m.gitAdded > 0 && m.gitDeleted == 0:
		gitColor = constants.Green
	case m.gitDeleted > 0 && m.gitAdded == 0:
		gitColor = constants.Red
	case m.gitAdded > 0 && m.gitDeleted > 0:
		gitColor = constants.Yellow
	default:
		gitColor = constants.Gray
	}
	gitSty := lipgloss.NewStyle().Foreground(gitColor)
	line2Right := whiteSty.Render(fmt.Sprintf("turn: 0 | %s ", m.branch)) + gitSty.Render(gitStr)

	row1 := footerRow(cw, line1Left, line1Right)
	row2 := footerRow(cw, line2Left, line2Right)

	footerContent := lipgloss.JoinVertical(lipgloss.Left, row1, row2)
	return lipgloss.NewStyle().PaddingLeft(1).Render(footerContent)
}
