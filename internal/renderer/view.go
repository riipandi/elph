package renderer

import (
	"fmt"
	"image/color"
	"path/filepath"
	"strings"
	"time"

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

	dimStyle       = lipgloss.NewStyle().Foreground(constants.DimText)
	valStyle       = lipgloss.NewStyle().Foreground(constants.BrightText)
	primarySty     = lipgloss.NewStyle().Foreground(constants.PrimaryText)
	primaryBoldSty = lipgloss.NewStyle().Foreground(constants.PrimaryText).Bold(true)
	sidSty         = lipgloss.NewStyle().Foreground(constants.DimText)
	yellowSty      = lipgloss.NewStyle().Foreground(constants.Yellow).Italic(true)
	metaSty        = lipgloss.NewStyle().Foreground(constants.DimText)
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
	if m.toolInteractDialogActive() {
		parts = append(parts, m.toolInteractChromeView())
	}
	if av := m.activityView(); av != "" {
		parts = append(parts, av)
	}
	if tv := m.todoPanelView(); tv != "" {
		parts = append(parts, tv)
	}
	parts = append(parts, m.inputChromeView(), m.footerView())
	return parts
}

func (m Model) renderedViewHeight() int {
	return lipgloss.Height(lipgloss.JoinVertical(lipgloss.Top, m.viewParts()...))
}

func (m Model) chromeHeight() int {
	h := lipgloss.Height(m.inputChromeView()) + lipgloss.Height(m.footerView())
	if m.toolInteractDialogActive() {
		h += lipgloss.Height(m.toolInteractChromeView())
	}
	if m.showsActivity() {
		h += lipgloss.Height(m.activityView())
	}
	h += m.todoPanelHeight()
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
		prefix := primaryBoldSty.Render(m.promptChar + " ")
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

	prefixEnd := m.streamPrefixEndIndex()
	if prefixEnd >= 0 &&
		m.layout.StreamPrefixUpTo == prefixEnd &&
		prefixEnd <= n {
		var b strings.Builder
		b.WriteString(m.layout.StreamPrefix)
		for i := prefixEnd; i < n; i++ {
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
	switch msg.kind {
	case constants.MessageAI:
		return renderAIMessage(width, msg.text, false, false)
	case constants.MessageDetail:
		return renderDetailMessage(width, collapsibleLabel(msg), msg.text, msg.detailExpanded, msg.detailStatus, msg.at, collapsibleRenderOpts{})
	case constants.MessageThinking:
		return renderThinkingMessage(width, collapsibleLabel(msg), msg.text, msg.detailExpanded, collapsibleRenderOpts{})
	case constants.MessageUser:
		return renderUserMessage(width, msg.text, msg.at)
	default:
		return renderStyledMessage(width, msg.kind, msg.text)
	}
}

func (m *Model) renderMessageAt(index int) string {
	msg := m.messages[index]
	width := m.messageAreaWidth()
	streaming := m.isStreamingMessageAt(index)

	opts := m.collapsibleRenderOpts(msg, index)
	if c := msg.renderCache; c.hit(width, streaming, len(msg.text), msg.detailExpanded, msg.detailStatus, msg.at, opts) {
		return c.output
	}

	var out string
	switch {
	case streaming && msg.kind != constants.MessageThinking:
		out = renderStreamingMessage(width, msg.kind, msg.text)
	case msg.kind == constants.MessageAI:
		out = renderAIMessage(width, msg.text, false, msg.glamourPending)
	case msg.kind == constants.MessageDetail:
		out = renderDetailMessage(width, collapsibleLabel(msg), msg.text, msg.detailExpanded, msg.detailStatus, msg.at, opts)
	case msg.kind == constants.MessageThinking:
		if opts.showLiveBody {
			out = renderThinkingLiveStream(width, collapsibleLabel(msg), msg.text, msg.detailExpanded, opts)
		} else {
			out = renderThinkingMessage(width, collapsibleLabel(msg), msg.text, msg.detailExpanded, opts)
		}
	case msg.kind == constants.MessageUser:
		out = renderUserMessage(width, msg.text, msg.at)
	default:
		out = renderStyledMessage(width, msg.kind, msg.text)
	}

	m.messages[index].renderCache = messageRenderCache{
		width:             width,
		sourceLen:         len(msg.text),
		streaming:         streaming,
		expanded:          msg.detailExpanded,
		detailStatus:      msg.detailStatus,
		atUnix:            messageAtUnix(msg.at),
		showStatusPreview: opts.showStatusPreview,
		showLiveBody:      opts.showLiveBody,
		spinnerFrame:      opts.spinnerFrame,
		output:            out,
	}
	return out
}

func aiContentWidth(blockWidth int) int {
	_, hPad := messageBlockPadding(constants.MessageAI)
	return max(blockWidth-2*hPad, 1)
}

func messageBlockPadding(kind constants.MessageKind) (vertical, horizontal int) {
	switch kind {
	case constants.MessageUser, constants.MessageTool, constants.MessageDetail, constants.MessageThinking:
		return 1, 2
	default:
		return 0, 1
	}
}

// aiMessageBottomPad adds breathing room below the last line of an AI reply
// without shifting spacing above the block.
const aiMessageBottomPad = 1

// aiParagraphGap is the blank line count inserted between prose paragraphs.
const aiParagraphGap = 1

func renderAIBlock(blockWidth int, body string, horizontalApplied bool) string {
	base := constants.MessageStyle(constants.MessageAI)
	if !horizontalApplied {
		_, hPad := messageBlockPadding(constants.MessageAI)
		base = base.PaddingLeft(hPad).PaddingRight(hPad)
	}
	lineStyle := base.Width(blockWidth)

	chunks := make([]string, 0, strings.Count(body, "\n\n")+1)
	for _, para := range strings.Split(body, "\n\n") {
		if para = strings.TrimSpace(para); para != "" {
			chunks = append(chunks, para)
		}
	}
	if len(chunks) == 0 {
		return ""
	}

	renderChunk := func(para string, padBottom bool) string {
		lines := strings.Split(para, "\n")
		for j, line := range lines {
			style := lineStyle
			if padBottom && j == len(lines)-1 {
				style = style.PaddingBottom(aiMessageBottomPad)
			}
			lines[j] = style.Render(line)
		}
		return strings.Join(lines, "\n")
	}

	blocks := make([]string, 0, len(chunks)*2-1)
	for i, para := range chunks {
		if i > 0 {
			for range aiParagraphGap {
				blocks = append(blocks, lineStyle.Render(""))
			}
		}
		blocks = append(blocks, renderChunk(para, i == len(chunks)-1))
	}
	return strings.Join(blocks, "\n")
}

// renderUserMessage paints a user chat input block with an optional timestamp line.
func renderUserMessage(width int, text string, at time.Time) string {
	vPad, hPad := messageBlockPadding(constants.MessageUser)
	style := constants.MessageStyle(constants.MessageUser)
	content := text
	if ts := formatMessageTimestamp(at); ts != "" {
		tsLine := lipgloss.NewStyle().
			Foreground(constants.DimText).
			Background(constants.UserMsgBg).
			Render(ts)
		content = text + "\n\n" + tsLine
	}
	return style.Padding(vPad, hPad).Width(width).Render(content)
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
	label := dimStyle.Render(" " + m.activityLabel())
	elapsed := ""
	if m.agent.Stopwatch.Running() {
		elapsed = dimStyle.Render(" · " + formatCompactElapsed(m.agent.Stopwatch.Elapsed()))
	}
	suffix := dimStyle.Render("...")
	return lipgloss.NewStyle().
		MarginTop(1).
		PaddingLeft(1).
		Width(m.width).
		Render(spinner + label + elapsed + suffix)
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
	body = overlayInputPasteTokens(body, m.input.Value(), m.inputPastes)
	if !m.inputScrollable() {
		return body
	}
	// Draw the scrollbar in the last column of each line so we do not reserve a
	// separate gutter column with visible trailing padding before the bar.
	return overlayInputScrollBar(body, m.inputScrollBarView(), m.layout.InputWidth)
}

func (m Model) inputChromeView() string {
	if m.modelsSyncDialogActive() {
		return m.modelsSyncDialogView()
	}
	if m.modelSelectorActive() {
		return m.modelSelectorChromeView()
	}

	pasteEditor := m.pasteEditorView()
	palette := m.commandPaletteView()
	inputBox := m.inputBoxView(palette != "" || pasteEditor != "")

	var overlays []string
	if pasteEditor != "" {
		overlays = append(overlays, pasteEditor)
	}
	if palette != "" {
		overlays = append(overlays, palette)
	}
	if len(overlays) > 0 {
		overlays = append(overlays, inputBox)
		return lipgloss.JoinVertical(lipgloss.Top, overlays...)
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
	if hint := m.pasteHintView(); hint != "" {
		inner = lipgloss.JoinVertical(lipgloss.Top, hint, inner)
	}
	if hint := m.attachmentHintView(); hint != "" {
		inner = lipgloss.JoinVertical(lipgloss.Top, hint, inner)
	}
	if m.showPromptPrefix {
		prefix := primaryBoldSty.Render(m.promptChar + " ")
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
	imgLabel := m.footerImageLabel()
	if !m.modelSupportsImage {
		imgLabel = dimStyle.Render(imgLabel)
	}
	line1Left := modelSty.Render(m.modelName) + metaSty.Render(fmt.Sprintf(" | %s | T: %s | %s", m.provider, m.thinkingLevel, imgLabel))

	ctxFrac := m.displayContextFraction()
	ctxColor := constants.ContextUsageColor(ctxFrac)
	ctxSty := lipgloss.NewStyle().Foreground(ctxColor)
	line1Right := ctxSty.Render(fmt.Sprintf("%s | %.1f%% (%s)", m.footerCostLabel(), ctxFrac*100, m.contextWindowLabel()))

	modeSty := lipgloss.NewStyle().Foreground(constants.ModeBorderColor(m.mode)).Bold(true)
	line2Left := primaryBoldSty.Render(wd) + sidSty.Render(fmt.Sprintf(" [%s] ", sidVal)) + modeSty.Render(string(m.mode))

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
	line2Right := primarySty.Render(fmt.Sprintf("turn: %d | %s ", m.turnCount, m.branch)) + gitSty.Render(gitStr)

	row1 := footerRow(cw, line1Left, line1Right)
	row2 := footerRow(cw, line2Left, line2Right)

	footerContent := lipgloss.JoinVertical(lipgloss.Left, row1, row2)
	return lipgloss.NewStyle().PaddingLeft(1).Render(footerContent)
}
