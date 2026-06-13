package renderer

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"charm.land/lipgloss/v2/compat"
	"github.com/riipandi/elph/internal/command"
	"github.com/riipandi/elph/internal/constants"
)

var (
	cmdPaletteSelected = lipgloss.NewStyle().Foreground(constants.Blue).Bold(true)
	cmdPaletteName     = lipgloss.NewStyle().Foreground(constants.White)
	// Lifted gray for selected summary — softer than command highlight.
	cmdPaletteSummarySelected = lipgloss.NewStyle().Foreground(compat.AdaptiveColor{
		Light: lipgloss.Color("#6B7280"),
		Dark:  lipgloss.Color("#9B9B9B"),
	})
)

func cmdPaletteBorder(mode constants.AgentMode) lipgloss.Style {
	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(constants.ModeBorderColor(mode)).
		BorderBottom(false).
		Padding(0, 1)
}

func (m Model) commandPaletteActive() bool {
	return len(m.cmdSuggestions) > 0 && m.slashQueryActive()
}

func (m Model) argPaletteActive() bool {
	return len(m.argSuggestions) > 0 && m.slashQueryActive()
}

func (m Model) inputPaletteActive() bool {
	return m.mentionPaletteActive() || m.commandPaletteActive() || m.argPaletteActive()
}

func (m Model) slashQueryActive() bool {
	return strings.HasPrefix(strings.TrimLeft(m.input.Value(), " \t"), "/")
}

func (m Model) slashQuery() string {
	val := strings.TrimLeft(m.input.Value(), " \t")
	if !strings.HasPrefix(val, "/") {
		return ""
	}
	query := strings.TrimPrefix(val, "/")
	if idx := strings.Index(query, " "); idx >= 0 {
		query = query[:idx]
	}
	return strings.ToLower(strings.TrimSpace(query))
}

func (m Model) syncInputSuggestions() (Model, tea.Cmd) {
	m = m.syncInputPlaceholder()

	if !m.input.Focused() {
		m.cmdSuggestions = nil
		m.cmdSuggestIndex = 0
		m.argSuggestions = nil
		m.argSuggestIndex = 0
		m.mentionSuggestions = nil
		m.mentionSuggestIndex = 0
		m.mentionFilterQuery = ""
		return m, nil
	}

	if m.slashQueryActive() {
		m.mentionSuggestions = nil
		m.mentionSuggestIndex = 0
		m.mentionFilterQuery = ""
		return m.syncSlashSuggestionsOnly(), nil
	}

	m.cmdSuggestions = nil
	m.cmdSuggestIndex = 0
	m.argSuggestions = nil
	m.argSuggestIndex = 0
	return m.syncMentionSuggestions()
}

func (m Model) syncSlashSuggestions() Model {
	m, _ = m.syncInputSuggestions()
	return m
}

func (m Model) syncSlashSuggestionsOnly() Model {
	cmd, argQuery, ok := command.ResolveInput(m.input.Value())
	if ok && len(cmd.Args) > 0 && m.argInputReady(cmd) {
		m.cmdSuggestions = nil
		m.cmdSuggestIndex = 0
		m.argSuggestions = append([]command.ArgChoice(nil), cmd.Args...)
		m.argSuggestIndex = command.ArgChoiceIndex(cmd.Args, argQuery)
		return m
	}

	m.argSuggestions = nil
	m.argSuggestIndex = 0
	m.cmdSuggestions = command.Suggest(m.slashQuery())
	if m.cmdSuggestIndex >= len(m.cmdSuggestions) {
		m.cmdSuggestIndex = 0
	}
	return m
}

func (m Model) argInputReady(cmd command.SlashCommand) bool {
	trimmed := strings.TrimLeft(m.input.Value(), " \t")
	if trimmed == "/"+cmd.Name {
		return true
	}
	return strings.Contains(trimmed, " ")
}

func (m Model) syncInputPlaceholder() Model {
	placeholder := ""
	cmd, argQuery, ok := command.ResolveInput(m.input.Value())
	if ok && len(cmd.Args) > 0 && argQuery == "" && m.argInputReady(cmd) {
		placeholder = command.ArgsHint(cmd.Args)
	}
	m.input.Placeholder = placeholder
	return m
}

func (m Model) applyCommandCompletion() Model {
	if len(m.cmdSuggestions) == 0 {
		return m
	}
	selected := m.cmdSuggestions[m.cmdSuggestIndex]
	m.input.SetValue(command.CompleteInput(selected))
	m = m.syncPromptPrefix()
	m = m.syncInputWidth()
	m = m.syncSlashSuggestions()
	return m
}

func (m Model) applyArgPreview() Model {
	if len(m.argSuggestions) == 0 {
		return m
	}
	cmd, _, ok := command.ResolveInput(m.input.Value())
	if !ok {
		return m
	}
	selected := m.argSuggestions[m.argSuggestIndex]
	m.input.SetValue(command.CompleteArgInput(cmd, selected))
	m = m.syncPromptPrefix()
	m = m.syncInputWidth()
	m = m.syncInputPlaceholder()
	return m
}

func (m Model) cycleArgSelection(delta int) Model {
	if len(m.argSuggestions) == 0 {
		return m
	}

	_, argQuery, ok := command.ResolveInput(m.input.Value())
	if !ok {
		return m
	}
	if strings.TrimSpace(argQuery) == "" {
		return m.applyArgPreview()
	}

	n := len(m.argSuggestions)
	m.argSuggestIndex = (m.argSuggestIndex+delta%n+n) % n
	return m.applyArgPreview()
}

func (m Model) handleInputPaletteKey(msg tea.KeyPressMsg) (Model, bool) {
	if m.mentionPaletteActive() {
		return m.handleMentionPaletteKey(msg)
	}
	return m.handleSlashPaletteKey(msg)
}

func (m Model) handleSlashPaletteKey(msg tea.KeyPressMsg) (Model, bool) {
	if m.argPaletteActive() {
		switch msg.String() {
		case "tab", "right":
			return m.cycleArgSelection(1), true
		case "shift+tab":
			return m.cycleArgSelection(-1), true
		case "up":
			return m.cycleArgSelection(-1), true
		case "down":
			return m.cycleArgSelection(1), true
		}
		return m, false
	}

	if !m.commandPaletteActive() {
		return m, false
	}

	switch msg.String() {
	case "tab", "right":
		return m.applyCommandCompletion(), true
	case "up":
		if len(m.cmdSuggestions) == 0 {
			return m, false
		}
		m.cmdSuggestIndex = (m.cmdSuggestIndex - 1 + len(m.cmdSuggestions)) % len(m.cmdSuggestions)
		return m, true
	case "down":
		if len(m.cmdSuggestions) == 0 {
			return m, false
		}
		m.cmdSuggestIndex = (m.cmdSuggestIndex + 1) % len(m.cmdSuggestions)
		return m, true
	}
	return m, false
}

func (m Model) commandPaletteView() string {
	if !m.inputPaletteActive() {
		return ""
	}

	if m.mentionPaletteActive() {
		return m.mentionPaletteView()
	}
	if m.argPaletteActive() {
		return m.argPaletteView()
	}
	return m.cmdPaletteView()
}

func (m Model) cmdPaletteView() string {
	nameColW := command.NameColumnWidth(m.cmdSuggestions, false)
	lines := make([]string, len(m.cmdSuggestions))
	for i, cmd := range m.cmdSuggestions {
		name, gap, summary := command.AlignedRow(cmd, nameColW, false)
		var summaryStyled string
		if i == m.cmdSuggestIndex {
			name = cmdPaletteSelected.Render(name)
			summaryStyled = cmdPaletteSummarySelected.Render(summary)
		} else {
			name = cmdPaletteName.Render(name)
			summaryStyled = dimStyle.Render(summary)
		}
		lines[i] = name + gap + summaryStyled
	}

	inner := strings.Join(lines, "\n")
	boxW := borderedChromeWidth(m.chromeOuterWidth())
	return cmdPaletteBorder(m.mode).Width(boxW).Render(inner)
}

func (m Model) argPaletteView() string {
	nameColW := command.ArgColumnWidth(m.argSuggestions)
	lines := make([]string, len(m.argSuggestions))
	for i, arg := range m.argSuggestions {
		name, gap, summary := command.AlignedArgRow(arg, nameColW)
		var summaryStyled string
		if i == m.argSuggestIndex {
			name = cmdPaletteSelected.Render(name)
			summaryStyled = cmdPaletteSummarySelected.Render(summary)
		} else {
			name = cmdPaletteName.Render(name)
			summaryStyled = dimStyle.Render(summary)
		}
		lines[i] = name + gap + summaryStyled
	}

	inner := strings.Join(lines, "\n")
	boxW := borderedChromeWidth(m.chromeOuterWidth())
	return cmdPaletteBorder(m.mode).Width(boxW).Render(inner)
}

func (m Model) commandPaletteHeight() int {
	if view := m.commandPaletteView(); view != "" {
		return lipgloss.Height(view)
	}
	return 0
}