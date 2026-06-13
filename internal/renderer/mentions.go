package renderer

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/mention"
)

type mentionIndexMsg struct {
	workDir string
	entries []mention.Entry
	err     error
}

func loadMentionIndex(workDir string) tea.Cmd {
	return func() tea.Msg {
		entries, err := mention.Index(workDir)
		return mentionIndexMsg{workDir: workDir, entries: entries, err: err}
	}
}

func (m Model) mentionPaletteActive() bool {
	return len(m.mentionSuggestions) > 0
}

func (m Model) inputCursorOffset() int {
	lines := strings.Split(m.input.Value(), "\n")
	line := m.input.Line()
	if line < 0 {
		line = 0
	}
	if line >= len(lines) {
		line = max(len(lines)-1, 0)
	}

	offset := 0
	for i := 0; i < line; i++ {
		offset += len(lines[i]) + 1
	}
	offset += m.input.LineInfo().CharOffset
	if offset > len(m.input.Value()) {
		offset = len(m.input.Value())
	}
	return offset
}

func (m Model) activeMention() (query string, start int, ok bool) {
	return mention.FindActive(m.input.Value(), m.inputCursorOffset())
}

func (m Model) syncMentionSuggestions() (Model, tea.Cmd) {
	if !m.input.Focused() || m.slashQueryActive() {
		m.mentionSuggestions = nil
		m.mentionSuggestIndex = 0
		m.mentionActiveQuery = ""
		m.mentionFilterQuery = ""
		return m, nil
	}

	query, _, ok := m.activeMention()
	if !ok {
		m.mentionSuggestions = nil
		m.mentionSuggestIndex = 0
		m.mentionActiveQuery = ""
		m.mentionFilterQuery = ""
		return m, nil
	}

	var cmd tea.Cmd
	if m.mentionIndexDir != m.workDir && !m.mentionIndexLoading {
		m.mentionIndexLoading = true
		cmd = loadMentionIndex(m.workDir)
	}

	if len(m.mentionIndex) > 0 && m.mentionIndexDir == m.workDir {
		filterQuery := query
		if _, preview := mention.MatchSuggestionIndex(m.mentionIndex, query); preview {
			filterQuery = m.mentionFilterQuery
		} else {
			m.mentionFilterQuery = query
		}

		m.mentionSuggestions = mention.Suggest(filterQuery, m.mentionIndex)
		if query != m.mentionActiveQuery {
			if idx, matched := mention.MatchSuggestionIndex(m.mentionSuggestions, query); matched {
				m.mentionSuggestIndex = idx
			} else {
				m.mentionSuggestIndex = 0
			}
		}
		if m.mentionSuggestIndex >= len(m.mentionSuggestions) {
			m.mentionSuggestIndex = 0
		}
		m.mentionActiveQuery = query
	} else {
		m.mentionSuggestions = nil
	}
	return m, cmd
}

func (m Model) applyMentionPreview() Model {
	if len(m.mentionSuggestions) == 0 {
		return m
	}

	_, start, ok := m.activeMention()
	if !ok {
		return m
	}

	selected := m.mentionSuggestions[m.mentionSuggestIndex]
	cursor := m.inputCursorOffset()
	m.input.SetValue(mention.Complete(m.input.Value(), start, cursor, selected))
	m = m.syncPromptPrefix()
	m = m.syncInputWidth()
	return m
}

func (m Model) confirmMention() Model {
	if len(m.mentionSuggestions) == 0 {
		return m
	}

	_, start, ok := m.activeMention()
	if !ok {
		return m
	}

	selected := m.mentionSuggestions[m.mentionSuggestIndex]
	cursor := m.inputCursorOffset()
	completed := mention.Complete(m.input.Value(), start, cursor, selected)
	if !strings.HasSuffix(completed, " ") {
		completed += " "
	}

	m.input.SetValue(completed)
	m.mentionSuggestions = nil
	m.mentionSuggestIndex = 0
	m.mentionActiveQuery = ""
	m.mentionFilterQuery = ""
	m = m.syncPromptPrefix()
	m = m.syncInputWidth()
	return m
}

func (m Model) moveMentionSelection(delta int) Model {
	if len(m.mentionSuggestions) == 0 {
		return m
	}
	n := len(m.mentionSuggestions)
	m.mentionSuggestIndex = (m.mentionSuggestIndex + delta%n + n) % n
	return m
}

func (m Model) cycleMentionSelection(delta int) Model {
	if len(m.mentionSuggestions) == 0 {
		return m
	}

	query, _, ok := m.activeMention()
	if !ok {
		return m
	}
	_, preview := mention.MatchSuggestionIndex(m.mentionSuggestions, query)
	if strings.TrimSpace(query) == "" || !preview {
		m = m.applyMentionPreview()
		if q, _, ok := m.activeMention(); ok {
			m.mentionActiveQuery = q
		}
		return m
	}

	n := len(m.mentionSuggestions)
	m.mentionSuggestIndex = (m.mentionSuggestIndex + delta%n + n) % n
	m = m.applyMentionPreview()
	if q, _, ok := m.activeMention(); ok {
		m.mentionActiveQuery = q
	}
	return m
}

func (m Model) handleMentionPaletteKey(msg tea.KeyPressMsg) (Model, bool) {
	if !m.mentionPaletteActive() {
		return m, false
	}

	switch msg.String() {
	case "enter":
		return m.confirmMention(), true
	case "tab", "right":
		return m.cycleMentionSelection(1), true
	case "shift+tab":
		return m.cycleMentionSelection(-1), true
	case "up":
		return m.moveMentionSelection(-1), true
	case "down":
		return m.moveMentionSelection(1), true
	}
	return m, false
}

func (m Model) mentionPaletteView() string {
	if !m.mentionPaletteActive() {
		return ""
	}

	nameColW := mention.NameColumnWidth(m.mentionSuggestions)
	lines := make([]string, len(m.mentionSuggestions))
	for i, entry := range m.mentionSuggestions {
		name, gap, summary := mention.AlignedRow(entry, nameColW)
		var summaryStyled string
		if i == m.mentionSuggestIndex {
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
