package renderer

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"charm.land/lipgloss/v2/compat"
	"github.com/riipandi/elph/internal/command"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
)

var (
	cmdPaletteSelected = lipgloss.NewStyle().Foreground(uiconst.Blue).Bold(true)
	cmdPaletteName     = lipgloss.NewStyle().Foreground(uiconst.PrimaryText)
	// Lifted gray for selected summary — softer than command highlight.
	cmdPaletteSummarySelected = lipgloss.NewStyle().Foreground(compat.AdaptiveColor{
		Light: lipgloss.Color("#6C6C6C"),
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
	if m.modelSelectorActive() {
		m = m.refreshModelSelectorItems()
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
		m = m.ensurePromptTemplates()
	}

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

func (m Model) commandContext() command.Context {
	catalog := m.session.Catalog
	base := command.Context{
		WorkDir:         m.workDir,
		SystemPrompt:    m.session.SystemPrompt,
		LogPath:         m.session.LogPath,
		RequestsLogPath: m.session.RequestsLogPath,
		Catalog:         catalog,
		ProviderID:      m.session.ProviderID,
		ModelID:         m.session.ModelID,
		ModelName:       m.session.ModelName,
		PromptTemplates: m.promptTemplates,
		Skills:          m.slashSkills,
	}
	if cmd, _, ok := command.ResolveInput(m.input.Value(), base); ok && cmd.Name == "model" {
		if reloaded, err := provider.LoadCatalog(""); err == nil && len(reloaded.Providers) > 0 {
			catalog = reloaded
		}
	}
	base.Catalog = catalog
	return base
}

func (m Model) syncSlashSuggestionsOnly() Model {
	ctx := m.commandContext()
	cmd, argQuery, ok := command.ResolveInput(m.input.Value(), ctx)
	args := command.EffectiveArgs(cmd, ctx)
	if ok && command.HasStructuredArgs(cmd, ctx) && m.argInputReady(cmd) {
		m.suggest.CmdSuggestions = nil
		m.suggest.CmdSuggestIndex = 0
		m.suggest.ArgSuggestions = command.SuggestArgs(cmd, ctx, argQuery)
		if argQuery != "" && command.ArgExactMatch(args, argQuery) {
			m.suggest.ArgSuggestions = append([]command.ArgChoice(nil), args...)
		}
		m.suggest.ArgSuggestIndex = command.ArgChoiceIndex(m.suggest.ArgSuggestions, argQuery)
		return m
	}

	if ok && !command.HasStructuredArgs(cmd, ctx) && m.argInputReady(cmd) {
		m.suggest.CmdSuggestions = nil
		m.suggest.CmdSuggestIndex = 0
		m.suggest.ArgSuggestions = nil
		m.suggest.ArgSuggestIndex = 0
		return m
	}

	m.suggest.ArgSuggestions = nil
	m.suggest.ArgSuggestIndex = 0
	m.suggest.CmdSuggestions = command.SuggestVisible(m.input.Value(), ctx)
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
	if m.modelSelectorActive() {
		m.input.Placeholder = m.modelSelectorPlaceholderText()
		return m
	}

	placeholder := ""
	ctx := m.commandContext()
	cmd, argQuery, ok := command.ResolveInput(m.input.Value(), ctx)
	if ok && argQuery == "" && m.argInputReady(cmd) {
		placeholder = command.InputPlaceholderHint(cmd, ctx)
	}
	m.input.Placeholder = placeholder
	return m
}

func (m Model) applyCommandCompletion() Model {
	if len(m.suggest.CmdSuggestions) == 0 {
		return m
	}
	selected := m.suggest.CmdSuggestions[m.suggest.CmdSuggestIndex]
	m.input.SetValue(command.CompleteInput(selected, m.commandContext()))
	m = m.syncPromptPrefix()
	m = m.syncInputWidth()
	m = m.syncSlashSuggestions()
	return m
}

func (m Model) applyArgPreview() Model {
	if len(m.suggest.ArgSuggestions) == 0 {
		return m
	}
	ctx := m.commandContext()
	cmd, _, ok := command.ResolveInput(m.input.Value(), ctx)
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

	ctx := m.commandContext()
	_, argQuery, ok := command.ResolveInput(m.input.Value(), ctx)
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

func (m Model) handleInputPaletteKey(msg tea.KeyPressMsg) (Model, tea.Cmd, bool) {
	if m.mentionPaletteActive() {
		m, ok := m.handleMentionPaletteKey(msg)
		return m, nil, ok
	}
	return m.handleSlashPaletteKey(msg)
}

func (m Model) confirmSlashCommand() (Model, tea.Cmd, bool) {
	if len(m.suggest.CmdSuggestions) == 0 {
		return m, nil, false
	}

	selected := m.suggest.CmdSuggestions[m.suggest.CmdSuggestIndex]
	ctx := m.commandContext()
	completed := command.CompleteInput(selected, ctx)

	if command.RequiresArgs(selected, ctx) {
		m.input.SetValue(completed)
		m = m.syncPromptPrefix()
		m = m.syncInputWidth()
		m = m.syncSlashSuggestions()
		return m, nil, true
	}

	return m.handleSlashCommand(completed)
}

func (m Model) confirmSlashArg() (Model, tea.Cmd, bool) {
	if len(m.suggest.ArgSuggestions) == 0 {
		return m, nil, false
	}

	ctx := m.commandContext()
	cmd, _, ok := command.ResolveInput(m.input.Value(), ctx)
	if !ok {
		return m, nil, false
	}

	selected := m.suggest.ArgSuggestions[m.suggest.ArgSuggestIndex]
	completed := command.CompleteArgInput(cmd, selected)
	return m.handleSlashCommand(completed)
}

func (m Model) handleSlashPaletteKey(msg tea.KeyPressMsg) (Model, tea.Cmd, bool) {
	if m.argPaletteActive() {
		switch msg.String() {
		case "enter":
			return m.confirmSlashArg()
		case "tab", "right":
			return m.cycleArgSelection(1), nil, true
		case "shift+tab":
			return m.cycleArgSelection(-1), nil, true
		case "up":
			return m.cycleArgSelection(-1), nil, true
		case "down":
			return m.cycleArgSelection(1), nil, true
		}
		return m, nil, false
	}

	if !m.commandPaletteActive() {
		return m, nil, false
	}

	switch msg.String() {
	case "enter":
		return m.confirmSlashCommand()
	case "tab", "right":
		return m.applyCommandCompletion(), nil, true
	case "up":
		if len(m.suggest.CmdSuggestions) == 0 {
			return m, nil, false
		}
		m.suggest.CmdSuggestIndex = (m.suggest.CmdSuggestIndex - 1 + len(m.suggest.CmdSuggestions)) % len(m.suggest.CmdSuggestions)
		return m, nil, true
	case "down":
		if len(m.suggest.CmdSuggestions) == 0 {
			return m, nil, false
		}
		m.suggest.CmdSuggestIndex = (m.suggest.CmdSuggestIndex + 1) % len(m.suggest.CmdSuggestions)
		return m, nil, true
	}
	return m, nil, false
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
	nameColW := command.PaletteNameColumnWidth(m.suggest.CmdSuggestions)
	rows := make([]paletteRow, len(m.suggest.CmdSuggestions))
	for i, cmd := range m.suggest.CmdSuggestions {
		name, _, summary := command.AlignedPaletteRow(cmd, nameColW)
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

func (m Model) handleCompactHistory(result command.Result) Model {
	history := m.session.History
	if len(history) == 0 {
		m = m.addDetailMessage("Compact history", "No conversation history to compact.")
		return m.syncLayout(true)
	}

	before := len(history)
	beforeBytes := 0
	for _, msg := range history {
		beforeBytes += len(msg.Content)
	}

	// Load settings for smart compaction
	prefs, err := settings.Load()
	if err != nil {
		prefs = settings.Settings{}
	}

	// Use settings for threshold
	threshold := agent.CompactionThreshold{
		MinMessages:  prefs.GetCompactMinMessages(),
		MinBytes:     prefs.GetCompactMinBytes(),
		MinTokens:    16 * 1024, // 16K tokens minimum
		ContextUsage: prefs.GetCompactContextUsage(),
	}

	// Smart compaction: skip if conversation is too small (unless manual override)
	if result.CompactRatio <= 0 && !agent.ShouldCompact(history, threshold) {
		m = m.addDetailMessage("Compact history", fmt.Sprintf(
			"Conversation too small to compact (%d messages, %s).",
			before, formatBytes(beforeBytes)))
		return m.syncLayout(true)
	}

	ratio := result.CompactRatio
	if ratio <= 0 || ratio >= 100 {
		ratio = prefs.CompactLimit()
	}

	// Use new compaction tracking
	tokensBefore := agent.EstimateTokens(beforeBytes)
	compactionResult := agent.CompactMessagesWithEntry(history, ratio, agent.ReasonManual, tokensBefore)
	m.session.ApplyHistoryWithCompaction(compactionResult)

	after := len(compactionResult.Messages)
	afterBytes := 0
	for _, msg := range compactionResult.Messages {
		afterBytes += len(msg.Content)
	}

	var body string
	if compactionResult.Entry != nil {
		body = fmt.Sprintf("Reduced: %d → %d messages (%s → %s)\nCompactions: %d",
			before, after,
			formatBytes(beforeBytes), formatBytes(afterBytes),
			m.session.CompactionCount)
	} else {
		body = fmt.Sprintf("Reduced: %d → %d messages (%s → %s)",
			before, after,
			formatBytes(beforeBytes), formatBytes(afterBytes))
	}
	m = m.addDetailMessage("Compact history", body)
	m.session.AppendLog("detail", body)
	return m.syncLayout(true)
}

func formatBytes(b int) string {
	switch {
	case b < 1<<10:
		return fmt.Sprintf("%dB", b)
	case b < 1<<20:
		return fmt.Sprintf("%.1fKB", float64(b)/(1<<10))
	default:
		return fmt.Sprintf("%.1fMB", float64(b)/(1<<20))
	}
}
