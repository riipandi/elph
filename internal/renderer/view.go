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

	dirLine := fmt.Sprintf("Directory:  %s", m.workDir)
	modelLine := fmt.Sprintf("Model:      %s", m.modelName)
	statsLine := fmt.Sprintf("Stats:      00 ext, 00 commands, 00 skills, 00 tools | Mode: %s", m.mode)

	logo := lipgloss.JoinVertical(lipgloss.Left,
		lipgloss.NewStyle().Foreground(special).Render(logoLine1),
		lipgloss.NewStyle().Foreground(special).Render(logoLine2),
	)

	content := lipgloss.JoinHorizontal(lipgloss.Top,
		lipgloss.NewStyle().MarginRight(2).Render(logo),
		lipgloss.JoinVertical(lipgloss.Left,
			header,
			"",
			lipgloss.NewStyle().Foreground(dimText).Render(dirLine),
			lipgloss.NewStyle().Foreground(dimText).Render(modelLine),
			lipgloss.NewStyle().Foreground(dimText).Render(statsLine),
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

	line1 := fmt.Sprintf("%s | opencode | T: high | IMG", m.modelName)
	line1right := "$0.00 | 0.0% (262k)"
	pad1 := w - lipgloss.Width(line1) - lipgloss.Width(line1right)
	if pad1 < 1 {
		pad1 = 1
	}

	line2 := fmt.Sprintf("%s [%s]", wd, m.sessionID)
	line2right := fmt.Sprintf("turn: 0 | %s [+00 -00]", m.branch)
	pad2 := w - lipgloss.Width(line2) - lipgloss.Width(line2right)
	if pad2 < 1 {
		pad2 = 1
	}

	s := lipgloss.NewStyle().Foreground(dimText)

	return footerStyle(w).Render(
		lipgloss.JoinVertical(lipgloss.Left,
			s.Render(line1+strings.Repeat(" ", pad1)+line1right),
			s.Render(line2+strings.Repeat(" ", pad2)+line2right),
		),
	)
}
