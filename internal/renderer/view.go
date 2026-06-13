package renderer

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/config"
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

	// Metadata lines: left-aligned to banner edge (no logo offset).
	meta := lipgloss.JoinVertical(lipgloss.Left,
		"",
		dimStyle.MaxWidth(metaW).Render(fmt.Sprintf("Directory:  %s", m.workDir)),
		dimStyle.MaxWidth(metaW).Render(fmt.Sprintf("Model:      %s [%s] (000 available)", m.modelName, m.provider)),
		dimStyle.MaxWidth(metaW).Render(fmt.Sprintf("Stats:      %d ext, %d commands, %d skills, %d tools", 0, 0, 0, 0)),
		dimStyle.MaxWidth(metaW).Render(fmt.Sprintf("MCP Server: %d/%d connected (%d tools)", 0, 0, 0)),
	)

	// Tip: word-wraps within available width.
	tipStyle := lipgloss.NewStyle().
		Foreground(dimText).
		Italic(true).
		Width(tipW)
	tip := tipStyle.Render("Tip: " + m.tip)

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
	m.input.SetWidth(w - 6)
	return inputStyle(w, m.mode).Render(m.input.View())
}

func (m Model) footerView() string {
	wd := filepath.Base(m.workDir)

	w := m.width
	cw := w - 2 // account for PaddingLeft(1)

	sid := m.sessionID
	if len(sid) > 16 {
		sid = sid[:16]
	}

	s := lipgloss.NewStyle().Foreground(dimText)

	line1Left := fmt.Sprintf("%s | %s | T: high | IMG", m.modelName, m.provider)
	line1Right := "$0.00 | 0.0% (262k)"

	line2Left := fmt.Sprintf("%s [%s]", wd, sid)
	line2Right := fmt.Sprintf("turn: 0 | %s [+00 -00]", m.branch)

	// Line 1: left takes remaining space after right, with gap between them.
	rightW1 := lipgloss.Width(line1Right)
	left1W := max(cw-rightW1, 0)
	left1 := s.Width(left1W).Render(line1Left)
	right1 := s.Render(line1Right)
	row1 := lipgloss.JoinHorizontal(lipgloss.Top, left1, right1)

	// Line 2: same approach.
	rightW2 := lipgloss.Width(line2Right)
	left2W := max(cw-rightW2, 0)
	left2 := s.Width(left2W).Render(line2Left)
	right2 := s.Render(line2Right)
	row2 := lipgloss.JoinHorizontal(lipgloss.Top, left2, right2)

	footerContent := lipgloss.JoinVertical(lipgloss.Left, row1, row2)

	return lipgloss.NewStyle().PaddingLeft(1).Render(footerContent)
}
