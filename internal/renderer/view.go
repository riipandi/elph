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

	versionLine := fmt.Sprintf("Welcome to Elph v%s", config.AppVersion)
	if config.BuildHash != "unknown" {
		versionLine = fmt.Sprintf("Welcome to Elph v%s (%s)", config.AppVersion, config.BuildHash[:7])
	}

	header := lipgloss.NewStyle().Bold(true).Render(versionLine)
	subtitle := lipgloss.NewStyle().Foreground(dimText).Render("Send /changelog to show version history.")

	dirLine := fmt.Sprintf("Directory:  %s", m.workDir)
	modelLine := fmt.Sprintf("Model:      %s [%s] (000 available)", m.modelName, m.provider)
	statsLine := fmt.Sprintf("Stats:      00 ext, 00 commands, 00 skills, 00 tools")
	mcpLine := fmt.Sprintf("MCP Server: 0/0 connected (000 tools)")

	logo := lipgloss.JoinVertical(lipgloss.Left,
		lipgloss.NewStyle().Foreground(special).Render(logoLine1),
		lipgloss.NewStyle().Foreground(special).Render(logoLine2),
	)

	content := lipgloss.JoinHorizontal(lipgloss.Top,
		lipgloss.NewStyle().MarginRight(2).Render(logo),
		lipgloss.JoinVertical(lipgloss.Left,
			header,
			subtitle,
			"",
			lipgloss.NewStyle().Foreground(dimText).Render(dirLine),
			lipgloss.NewStyle().Foreground(dimText).Render(modelLine),
			lipgloss.NewStyle().Foreground(dimText).Render(statsLine),
			lipgloss.NewStyle().Foreground(dimText).Render(mcpLine),
			"",
			lipgloss.NewStyle().Foreground(dimText).Italic(true).Render("Tip: "+m.tip),
		),
	)

	return bannerStyle(w).Render(content)
}

func (m Model) streamView() string {
	if len(m.messages) == 0 {
		return lipgloss.NewStyle().Foreground(dimText).Render("MCP: 0 servers connected (000 tools)")
	}

	var b strings.Builder
	for _, msg := range m.messages {
		b.WriteString(msg)
		b.WriteString("\n")
	}
	return strings.TrimRight(b.String(), "\n")
}

func (m Model) inputView() string {
	w := m.width
	m.input.Width = w - 6
	borderColor := modeBorderColor(m.mode)
	m.input.PromptStyle = lipgloss.NewStyle().Foreground(borderColor)
	return inputStyle(w, m.mode).Render(m.input.View())
}

func (m Model) footerView() string {
	w := m.width
	wd := filepath.Base(m.workDir)

	// Content width: footer has no border, just plain text
	cw := w

	// Truncate session ID to fit (keep prefix + first 8 chars)
	sid := m.sessionID
	if len(sid) > 16 {
		sid = sid[:16]
	}

	s := lipgloss.NewStyle().Foreground(dimText)

	line1Left := fmt.Sprintf("%s | %s | T: high | IMG", m.modelName, m.provider)
	line1Right := "$0.00 | 0.0% (262k)"

	line2Left := fmt.Sprintf("%s [%s]", wd, sid)
	line2Right := fmt.Sprintf("turn: 0 | %s [+00 -00]", m.branch)

	// Build each row: left takes remaining space, right is flush to edge
	rightW1 := lipgloss.Width(line1Right)
	left1 := s.Width(max(cw-rightW1, 0)).Render(line1Left)
	row1 := lipgloss.JoinHorizontal(lipgloss.Top, left1, s.Render(line1Right))

	rightW2 := lipgloss.Width(line2Right)
	left2 := s.Width(max(cw-rightW2, 0)).Render(line2Left)
	row2 := lipgloss.JoinHorizontal(lipgloss.Top, left2, s.Render(line2Right))

	return lipgloss.JoinVertical(lipgloss.Left, row1, row2)
}
