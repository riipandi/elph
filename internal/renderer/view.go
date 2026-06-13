package renderer

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/config"
	"github.com/riipandi/elph/internal/constants"
)

// ─── View ────────────────────────────────────────────────────────────────────

func (m Model) View() string {
	if m.quitting {
		return ""
	}
	if !m.ready {
		return "\n  Initializing..."
	}

	inputView := m.inputView()
	footerView := m.footerView()

	parts := []string{
		inputView,
		footerView,
	}

	return lipgloss.JoinVertical(lipgloss.Top, parts...)
}

// ─── Stream View ─────────────────────────────────────────────────────────────

func (m Model) streamView() string {
	var b strings.Builder

	// Banner (scrolls with content)
	b.WriteString(m.bannerView())
	b.WriteString("\n\n")

	// Messages
	for _, msg := range m.messages {
		b.WriteString(m.renderMessage(msg))
		b.WriteString("\n")
	}

	return b.String()
}

func (m Model) renderMessage(msg message) string {
	w := max(m.width-2, 1)
	switch msg.kind {
	case msgUser, msgAI:
		return padLine(w, msg.text)
	case msgSystem:
		return padLine(w, lipgloss.NewStyle().Foreground(constants.DimText).Render(msg.text))
	}
	return msg.text
}

// padLine wraps content with padding.
func padLine(width int, content string) string {
	return lipgloss.NewStyle().Padding(0, 1).Width(width).Render(content)
}

// ─── Sub-views ───────────────────────────────────────────────────────────────

func (m Model) bannerView() string {
	w := m.width

	// Pre-compute available widths for line-clamp and wrap.
	metaW := max(w-6, 20)
	tipW := max(w-6, 10)

	versionLine := fmt.Sprintf("Welcome to Elph v%s", config.AppVersion)
	if config.BuildHash != "unknown" {
		versionLine = fmt.Sprintf("Welcome to Elph v%s (%s)", config.AppVersion, config.BuildHash[:7])
	}

	header := lipgloss.NewStyle().Bold(true).Render(versionLine)
	subtitle := lipgloss.NewStyle().Foreground(constants.DimText).MaxWidth(metaW).Render("Send /changelog to show version history.")

	logo := lipgloss.JoinVertical(lipgloss.Left,
		lipgloss.NewStyle().Foreground(constants.GreenLt).Render(logoLine1),
		lipgloss.NewStyle().Foreground(constants.GreenLt).Render(logoLine2),
	)

	// Top section: logo + header/subtitle side by side.
	topSection := lipgloss.JoinHorizontal(lipgloss.Top,
		lipgloss.NewStyle().MarginRight(2).Render(logo),
		lipgloss.JoinVertical(lipgloss.Left, header, subtitle),
	)

	dimStyle := lipgloss.NewStyle().Foreground(constants.DimText)
	valStyle := lipgloss.NewStyle().Foreground(constants.BrightText)

	// Metadata lines: left-aligned to banner edge (no logo offset).
	meta := lipgloss.JoinVertical(lipgloss.Left,
		"",
		dimStyle.MaxWidth(metaW).Render("Directory:  ")+valStyle.Render(m.workDir),
		dimStyle.MaxWidth(metaW).Render("Model:      ")+valStyle.Render(fmt.Sprintf("%s [%s] (000 available)", m.modelName, m.provider)),
		dimStyle.MaxWidth(metaW).Render("Stats:      ")+valStyle.Render(fmt.Sprintf("%d ext, %d commands, %d skills, %d tools", 0, 0, 0, 0)),
		dimStyle.MaxWidth(metaW).Render("MCP:        ")+valStyle.Render(fmt.Sprintf("%d/%d connected (%d tools)", 0, 0, 0)),
	)

	// Tip: word-wraps within available width.
	tipLabel := lipgloss.NewStyle().Foreground(constants.Yellow).Italic(true).Render("Tip:")
	tipBody := lipgloss.NewStyle().Foreground(constants.DimText).Italic(true).Render(" " + m.tip)
	tip := lipgloss.NewStyle().Width(tipW).Render(tipLabel + tipBody)

	content := lipgloss.JoinVertical(lipgloss.Left, topSection, meta, "", tip)

	return bannerStyle(w).Render(content)
}

func (m Model) inputView() string {
	w := m.width
	if m.showPromptPrefix {
		prefix := lipgloss.NewStyle().Foreground(constants.White).Bold(true).Render(m.promptChar + " ")
		prefixW := lipgloss.Width(prefix)
		m.input.SetWidth(w - 6 - prefixW)
		return inputStyle(w, m.mode).Render(prefix + m.input.View())
	}
	m.input.SetWidth(w - 6)
	return inputStyle(w, m.mode).Render(m.input.View())
}

func (m Model) footerView() string {
	wd := filepath.Base(m.workDir)
	sid := m.sessionID.Suffix()

	w := m.width
	cw := w - 2 // account for PaddingLeft(1)

	// --- Line 1 left: model (thinking color) | provider | T: level | IMG ---
	modelSty := lipgloss.NewStyle().Foreground(constants.ThinkingColor(m.thinkingLevel))
	metaSty := lipgloss.NewStyle().Foreground(constants.DimText)
	line1LeftRendered := modelSty.Render(m.modelName) + metaSty.Render(fmt.Sprintf(" | %s | T: %s | IMG", m.provider, m.thinkingLevel))

	// --- Line 1 right: cost | context% (dynamic color) ---
	ctxColor := constants.ContextUsageColor(m.contextUsed)
	ctxSty := lipgloss.NewStyle().Foreground(ctxColor)
	line1RightRendered := ctxSty.Render(fmt.Sprintf("$0.00 | %.1f%% (262k)", m.contextUsed*100))

	// --- Line 2 left: dir (white) [session] mode (mode color) ---
	dirSty := lipgloss.NewStyle().Foreground(constants.White)
	sidSty := lipgloss.NewStyle().Foreground(constants.DimText)
	modeSty := lipgloss.NewStyle().Foreground(constants.ModeBorderColor(m.mode)).Bold(true)
	line2LeftRendered := dirSty.Render(wd) + sidSty.Render(fmt.Sprintf(" [%s] ", sid)) + modeSty.Render(string(m.mode))

	// --- Line 2 right: turn | branch [+add -del] (white) ---
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
	line2RightRendered := lipgloss.NewStyle().Foreground(constants.White).Render(fmt.Sprintf("turn: 0 | %s ", m.branch)) + gitSty.Render(gitStr)

	// Line 1: left takes remaining space, right flush to edge.
	rightW1 := lipgloss.Width(line1RightRendered)
	left1W := max(cw-rightW1, 0)
	left1 := metaSty.Width(left1W).Render(line1LeftRendered)
	row1 := lipgloss.JoinHorizontal(lipgloss.Top, left1, line1RightRendered)

	// Line 2: same approach.
	rightW2 := lipgloss.Width(line2RightRendered)
	left2W := max(cw-rightW2, 0)
	left2 := metaSty.Width(left2W).Render(line2LeftRendered)
	row2 := lipgloss.JoinHorizontal(lipgloss.Top, left2, line2RightRendered)

	footerContent := lipgloss.JoinVertical(lipgloss.Left, row1, row2)
	return lipgloss.NewStyle().PaddingLeft(1).Render(footerContent)
}
