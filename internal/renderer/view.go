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

	bannerView := m.bannerView()
	inputView := m.inputView()
	footerView := m.footerView()

	bannerH := lipgloss.Height(bannerView)
	inputH := lipgloss.Height(inputView)
	footerH := lipgloss.Height(footerView)
	gaps := 2

	vpHeight := m.height - bannerH - inputH - footerH - gaps
	if vpHeight < 1 {
		vpHeight = 1
	}

	m.vp.Width = m.width
	m.vp.Height = vpHeight
	m.vp.SetContent(m.streamView())

	parts := []string{
		bannerView,
		"",
		m.vp.View(),
		"",
		inputView,
		footerView,
	}

	return lipgloss.JoinVertical(lipgloss.Top, parts...)
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
	subtitle := lipgloss.NewStyle().Foreground(dimText).MaxWidth(metaW).Render("Send /changelog to show version history.")

	logo := lipgloss.JoinVertical(lipgloss.Left,
		lipgloss.NewStyle().Foreground(special).Render(logoLine1),
		lipgloss.NewStyle().Foreground(special).Render(logoLine2),
	)

	// Top section: logo + header/subtitle side by side.
	topSection := lipgloss.JoinHorizontal(lipgloss.Top,
		lipgloss.NewStyle().MarginRight(2).Render(logo),
		lipgloss.JoinVertical(lipgloss.Left,
			header,
			subtitle,
		),
	)

	dimStyle := lipgloss.NewStyle().Foreground(dimText)
	valStyle := lipgloss.NewStyle().Foreground(brightText)

	// Metadata lines: left-aligned to banner edge (no logo offset).
	meta := lipgloss.JoinVertical(lipgloss.Left,
		"",
		dimStyle.MaxWidth(metaW).Render("Directory:  ")+valStyle.Render(m.workDir),
		dimStyle.MaxWidth(metaW).Render("Model:      ")+valStyle.Render(fmt.Sprintf("%s [%s] (000 available)", m.modelName, m.provider)),
		dimStyle.MaxWidth(metaW).Render("Stats:      ")+valStyle.Render(fmt.Sprintf("%d ext, %d commands, %d skills, %d tools", 0, 0, 0, 0)),
		dimStyle.MaxWidth(metaW).Render("MCP:        ")+valStyle.Render(fmt.Sprintf("%d/%d connected (%d tools)", 0, 0, 0)),
	)

	// Tip: word-wraps within available width.
	tipLabel := lipgloss.NewStyle().Foreground(yellowCol).Italic(true).Render("Tip:")
	tipBody := lipgloss.NewStyle().Foreground(dimText).Italic(true).Render(" " + m.tip)
	tip := lipgloss.NewStyle().Width(tipW).Render(tipLabel + tipBody)

	content := lipgloss.JoinVertical(lipgloss.Left,
		topSection,
		meta,
		"",
		tip,
	)

	return bannerStyle(w).Render(content)
}

func (m Model) streamView() string {
	var b strings.Builder
	for i, msg := range m.messages {
		if i > 0 {
			b.WriteString("\n")
		}
		switch msg.kind {
		case msgUser:
			b.WriteString(lipgloss.NewStyle().Foreground(userPipeCol).Render("|"))
			b.WriteString(" ")
			b.WriteString(msg.text)
		case msgAI:
			b.WriteString(lipgloss.NewStyle().Foreground(aiPipeCol).Render("|"))
			b.WriteString(" ")
			b.WriteString(msg.text)
		case msgSystem:
			b.WriteString(lipgloss.NewStyle().Foreground(highlight).Render("> "))
			b.WriteString(msg.text)
		}
	}
	return b.String()
}

func (m Model) inputView() string {
	w := m.width
	prefix := lipgloss.NewStyle().Foreground(whiteCol).Bold(false).Render(m.promptChar + " ")
	prefixW := lipgloss.Width(prefix)
	m.input.SetWidth(w - 6 - prefixW)
	return inputStyle(w, m.mode).Render(prefix + m.input.View())
}

func (m Model) footerView() string {
	wd := filepath.Base(m.workDir)

	w := m.width
	cw := w - 2 // account for PaddingLeft(1)

	sid := m.sessionID
	if len(sid) > 16 {
		sid = sid[:16]
	}

	// --- Line 1 left: model (thinking color) | provider | T: level | IMG ---
	modelSty := lipgloss.NewStyle().Foreground(constants.ThinkingColor(m.thinkingLevel))
	metaSty := lipgloss.NewStyle().Foreground(dimText)
	line1LeftRendered := modelSty.Render(m.modelName) + metaSty.Render(fmt.Sprintf(" | %s | T: %s | IMG", m.provider, m.thinkingLevel))

	// --- Line 1 right: cost | context% (dynamic color) ---
	ctxColor := ContextUsageColor(m.contextUsed)
	ctxSty := lipgloss.NewStyle().Foreground(ctxColor)
	line1RightRendered := ctxSty.Render(fmt.Sprintf("$0.00 | %.1f%% (262k)", m.contextUsed*100))

	// --- Line 2 left: dir (white) [session] mode (mode color) ---
	dirSty := lipgloss.NewStyle().Foreground(whiteCol)
	sidSty := lipgloss.NewStyle().Foreground(dimText)
	modeSty := lipgloss.NewStyle().Foreground(modeBorderColor(m.mode)).Bold(true)
	line2LeftRendered := dirSty.Render(wd) + sidSty.Render(fmt.Sprintf(" [%s] ", sid)) + modeSty.Render(string(m.mode))

	// --- Line 2 right: turn | branch [+add -del] (white) ---
	gitStr := "[-]"
	if m.gitAdded > 0 || m.gitDeleted > 0 {
		gitStr = fmt.Sprintf("[+%d -%d]", m.gitAdded, m.gitDeleted)
	}
	var gitColor lipgloss.Color
	switch {
	case m.gitAdded > 0 && m.gitDeleted == 0:
		gitColor = lipgloss.Color("#22C55E") // green: only additions
	case m.gitDeleted > 0 && m.gitAdded == 0:
		gitColor = lipgloss.Color("#EF4444") // red: only deletions
	case m.gitAdded > 0 && m.gitDeleted > 0:
		gitColor = lipgloss.Color("#EAB308") // yellow: mixed changes
	default:
		gitColor = lipgloss.Color("#6B7280") // gray: no changes
	}
	gitSty := lipgloss.NewStyle().Foreground(gitColor)
	line2RightRendered := lipgloss.NewStyle().Foreground(whiteCol).Render(fmt.Sprintf("turn: 0 | %s ", m.branch)) + gitSty.Render(gitStr)

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
