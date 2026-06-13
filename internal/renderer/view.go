package renderer

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/config"
	"github.com/riipandi/elph/internal/constants"
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

// cachedInputBorder returns a border style for the given mode.
func cachedInputBorder(m constants.AgentMode) lipgloss.Style {
	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(constants.ModeBorderColor(m)).
		Padding(0, 1)
}

// ─── View ────────────────────────────────────────────────────────────────────

func (m Model) View() string {
	if m.quitting {
		return ""
	}
	if !m.ready {
		return "\n  Initializing..."
	}

	return lipgloss.JoinVertical(lipgloss.Top,
		m.content.View(),
		m.inputView(),
		m.footerView(),
	)
}

// syncLayout sizes chrome and viewport. Rebuilds scrollable content only when
// dirty. When follow is true the viewport scrolls to the newest lines.
func (m Model) syncLayout(follow bool) Model {
	if !m.ready || m.width <= 0 || m.height <= 0 {
		return m
	}

	m = m.syncInputWidth()

	atBottom := m.content.AtBottom()
	chromeH := lipgloss.Height(m.inputView()) + lipgloss.Height(m.footerView())
	vpH := max(m.height-chromeH, 1)

	sizeChanged := m.content.Width != m.width || m.content.Height != vpH
	m.content.Width = m.width
	m.content.Height = vpH
	m.chromeH = chromeH

	if m.contentDirty || sizeChanged {
		m.content.SetContent(m.contentView())
		m.contentDirty = false
	}

	if follow || atBottom {
		m.content.GotoBottom()
	}

	return m
}

func (m Model) syncInputWidth() Model {
	w := m.width
	inputW := w - 6
	if m.showPromptPrefix {
		prefix := lipgloss.NewStyle().Foreground(constants.White).Bold(true).Render(m.promptChar + " ")
		inputW -= lipgloss.Width(prefix)
	}
	if inputW < 1 {
		inputW = 1
	}
	m.inputWidth = inputW
	m.input.SetWidth(inputW)
	return m
}

// contentView is the full scrollable region: banner + message history.
func (m Model) contentView() string {
	var b strings.Builder

	b.WriteString(m.bannerView())
	if len(m.messages) > 0 {
		b.WriteString("\n\n")
		for i, msg := range m.messages {
			if i > 0 {
				b.WriteString("\n")
			}
			b.WriteString(m.renderMessage(msg))
		}
	}

	return b.String()
}

func (m Model) renderMessage(msg message) string {
	w := max(m.width-2, 1)
	switch msg.kind {
	case msgUser, msgAI:
		return padLine(w, msg.text)
	case msgSystem:
		return padLine(w, dimStyle.Render(msg.text))
	}
	return msg.text
}

// padLine wraps content with padding.
func padLine(width int, content string) string {
	return lipgloss.NewStyle().Padding(0, 1).Width(width).Render(content)
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
// flush to the edge.
func footerRow(contentW int, left, right string) string {
	rightW := lipgloss.Width(right)
	if rightW >= contentW {
		return clampLine(contentW, right)
	}
	leftW := contentW - rightW
	return lipgloss.JoinHorizontal(lipgloss.Top, clampLine(leftW, left), right)
}

// ─── Sub-views ───────────────────────────────────────────────────────────────

func (m Model) bannerView() string {
	w := m.width
	innerW := bannerContentWidth(w)

	// TODO: replace with actual value
	updateAvailable := false

	versionLine := fmt.Sprintf("Welcome to %s v%s", config.AppName, config.AppVersion)
	if updateAvailable {
		updateNotice := lipgloss.NewStyle().Foreground(constants.Yellow).Italic(true).Bold(false).Render("(update available)")
		versionLine = fmt.Sprintf("Welcome to %s v%s %s", config.AppName, config.AppVersion, updateNotice)
	}

	logo := lipgloss.JoinVertical(lipgloss.Left,
		lipgloss.NewStyle().Foreground(constants.GreenLt).Render(logoLine1),
		lipgloss.NewStyle().Foreground(constants.GreenLt).Render(logoLine2),
	)
	logoBlock := lipgloss.NewStyle().MarginRight(2).Render(logo)
	topW := max(innerW-lipgloss.Width(logoBlock), 10)

	header := clampLine(topW, lipgloss.NewStyle().Bold(true).Render(versionLine))
	subtitle := clampLine(topW, dimStyle.Render("Send /changelog to show version history."))

	topSection := lipgloss.JoinHorizontal(lipgloss.Top, logoBlock, lipgloss.JoinVertical(lipgloss.Left, header, subtitle))

	meta := lipgloss.JoinVertical(lipgloss.Left,
		"",
		metaLine(innerW, "Directory:  ", m.workDir),
		metaLine(innerW, "Model:      ", fmt.Sprintf("%s [%s] (000 available)", m.modelName, m.provider)),
		metaLine(innerW, "Stats:      ", fmt.Sprintf("%d exts, %d commands, %d skills, %d tools", 0, 0, 0, 0)),
		metaLine(innerW, "MCP Server: ", fmt.Sprintf("%d/%d connected (%d tools)", 0, 0, 0)),
	)

	tipBody := dimStyle.Italic(true).Render(" " + m.tip)
	tip := wrapLine(innerW, yellowSty.Render("Tip:")+tipBody)

	content := lipgloss.JoinVertical(lipgloss.Left, topSection, meta, "", tip)

	return cachedBannerBorder.Width(w - 2).Render(content)
}

func (m Model) inputView() string {
	w := m.width
	border := cachedInputBorder(m.mode)
	if m.showPromptPrefix {
		prefix := lipgloss.NewStyle().Foreground(constants.White).Bold(true).Render(m.promptChar + " ")
		return border.Width(w - 2).Render(prefix + m.input.View())
	}
	return border.Width(w - 2).Render(m.input.View())
}

func (m Model) footerView() string {
	wd := filepath.Base(m.workDir)
	sidVal := m.sessionID.Suffix()

	cw := footerContentWidth(m.width)

	modelSty := lipgloss.NewStyle().Foreground(constants.ThinkingColor(m.thinkingLevel))
	line1Left := modelSty.Render(m.modelName) + metaSty.Render(fmt.Sprintf(" | %s | T: %s | IMG", m.provider, m.thinkingLevel))

	ctxColor := constants.ContextUsageColor(m.contextUsed)
	ctxSty := lipgloss.NewStyle().Foreground(ctxColor)
	line1Right := ctxSty.Render(fmt.Sprintf("$0.00 | %.1f%% (262k)", m.contextUsed*100))

	modeSty := lipgloss.NewStyle().Foreground(constants.ModeBorderColor(m.mode)).Bold(true)
	line2Left := whiteBoldSty.Render(wd) + sidSty.Render(fmt.Sprintf(" [%s] ", sidVal)) + modeSty.Render(string(m.mode))

	gitStr := "[-]"
	if m.gitAdded > 0 || m.gitDeleted > 0 {
		gitStr = fmt.Sprintf("[+%d -%d]", m.gitAdded, m.gitDeleted)
	}
	var gitColor lipgloss.Color
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