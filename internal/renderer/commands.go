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

func (m Model) commandPaletteActive() bool {
	return len(m.suggest.CmdSuggestions) > 0 && m.slashQueryActive()
}

func (m Model) argPaletteActive() bool {
	return len(m.suggest.ArgSuggestions) > 0 && m.slashQueryActive()
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
		m.suggest.CmdSuggestions = nil
		m.suggest.CmdSuggestIndex = 0
		m.suggest.ArgSuggestions = nil
		m.suggest.ArgSuggestIndex = 0
		m.suggest.MentionSuggestions = nil
		m.suggest.MentionSuggestIndex = 0
		m.suggest.MentionFilterQuery = ""
		return m, nil
	}

	if m.slashQueryActive() {
		m.suggest.MentionSuggestions = nil
		m.suggest.MentionSuggestIndex = 0
		m.suggest.MentionFilterQuery = ""
		return m.syncSlashSuggestionsOnly(), nil
	}

	m.suggest.CmdSuggestions = nil
	m.suggest.CmdSuggestIndex = 0
	m.suggest.ArgSuggestions = nil
	m.suggest.ArgSuggestIndex = 0
	return m.syncMentionSuggestions()
}

func (m Model) syncSlashSuggestions() Model {
	m, _ = m.syncInputSuggestions()
	return m
}

func (m Model) syncSlashSuggestionsOnly() Model {
	cmd, argQuery, ok := command.ResolveInput(m.input.Value())
	if ok && len(cmd.Args) > 0 && m.argInputReady(cmd) {
		m.suggest.CmdSuggestions = nil
		m.suggest.CmdSuggestIndex = 0
		m.suggest.ArgSuggestions = command.SuggestArgs(cmd, argQuery)
		if argQuery != "" && command.ArgExactMatch(cmd.Args, argQuery) {
			m.suggest.ArgSuggestions = append([]command.ArgChoice(nil), cmd.Args...)
		}
		m.suggest.ArgSuggestIndex = command.ArgChoiceIndex(m.suggest.ArgSuggestions, argQuery)
		return m
	}

	m.suggest.ArgSuggestions = nil
	m.suggest.ArgSuggestIndex = 0
	m.suggest.CmdSuggestions = command.Suggest(m.slashQuery())
	if m.suggest.CmdSuggestIndex >= len(m.suggest.CmdSuggestions) {
		m.suggest.CmdSuggestIndex = 0
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
	if len(m.suggest.CmdSuggestions) == 0 {
		return m
	}
	selected := m.suggest.CmdSuggestions[m.suggest.CmdSuggestIndex]
	m.input.SetValue(command.CompleteInput(selected))
	m = m.syncPromptPrefix()
	m = m.syncInputWidth()
	m = m.syncSlashSuggestions()
	return m
}

func (m Model) applyArgPreview() Model {
	if len(m.suggest.ArgSuggestions) == 0 {
		return m
	}
	cmd, _, ok := command.ResolveInput(m.input.Value())
	if !ok {
		return m
	}
	selected := m.suggest.ArgSuggestions[m.suggest.ArgSuggestIndex]
	m.input.SetValue(command.CompleteArgInput(cmd, selected))
	m = m.syncPromptPrefix()
	m = m.syncInputWidth()
	m = m.syncInputPlaceholder()
	return m
}

func (m Model) cycleArgSelection(delta int) Model {
	if len(m.suggest.ArgSuggestions) == 0 {
		return m
	}

	_, argQuery, ok := command.ResolveInput(m.input.Value())
	if !ok {
		return m
	}
	if strings.TrimSpace(argQuery) == "" {
		return m.applyArgPreview()
	}

	n := len(m.suggest.ArgSuggestions)
	m.suggest.ArgSuggestIndex = (m.suggest.ArgSuggestIndex + delta%n + n) % n
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
		if len(m.suggest.CmdSuggestions) == 0 {
			return m, false
		}
		m.suggest.CmdSuggestIndex = (m.suggest.CmdSuggestIndex - 1 + len(m.suggest.CmdSuggestions)) % len(m.suggest.CmdSuggestions)
		return m, true
	case "down":
		if len(m.suggest.CmdSuggestions) == 0 {
			return m, false
		}
		m.suggest.CmdSuggestIndex = (m.suggest.CmdSuggestIndex + 1) % len(m.suggest.CmdSuggestions)
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
	nameColW := command.NameColumnWidth(m.suggest.CmdSuggestions, false)
	rows := make([]paletteRow, len(m.suggest.CmdSuggestions))
	for i, cmd := range m.suggest.CmdSuggestions {
		name, _, summary := command.AlignedRow(cmd, nameColW, false)
		rows[i] = paletteRow{name: name, summary: summary}
	}
	return m.renderPaletteRows(rows, m.suggest.CmdSuggestIndex, nameColW)
}

func (m Model) argPaletteView() string {
	nameColW := command.ArgColumnWidth(m.suggest.ArgSuggestions)
	rows := make([]paletteRow, len(m.suggest.ArgSuggestions))
	for i, arg := range m.suggest.ArgSuggestions {
		name, _, summary := command.AlignedArgRow(arg, nameColW)
		rows[i] = paletteRow{name: name, summary: summary}
	}
	return m.renderPaletteRows(rows, m.suggest.ArgSuggestIndex, nameColW)
}

func (m Model) commandPaletteHeight() int {
	if view := m.commandPaletteView(); view != "" {
		return lipgloss.Height(view)
	}
	return 0
}
