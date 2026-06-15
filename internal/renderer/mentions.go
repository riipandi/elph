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
	return len(m.suggest.MentionSuggestions) > 0
}

func inputCursorByteCol(line string, col int) int {
	runes := []rune(line)
	if col > len(runes) {
		col = len(runes)
	}
	if col <= 0 {
		return 0
	}
	return len(string(runes[:col]))
}

func (m Model) inputCursorOffset() int {
	val := m.input.Value()
	lines := strings.Split(val, "\n")
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
	offset += inputCursorByteCol(lines[line], m.input.Column())
	if offset > len(val) {
		offset = len(val)
	}
	return offset
}

func (m Model) activeMention() (query string, start int, ok bool) {
	return mention.FindActive(m.input.Value(), m.inputCursorOffset())
}

func (m Model) syncMentionSuggestions() (Model, tea.Cmd) {
	if !m.input.Focused() || m.slashQueryActive() {
		m.suggest.MentionSuggestions = nil
		m.suggest.MentionSuggestIndex = 0
		m.suggest.MentionActiveQuery = ""
		m.suggest.MentionFilterQuery = ""
		m.suggest.MentionUserSelected = false
		return m, nil
	}

	query, _, ok := m.activeMention()
	if !ok {
		m.suggest.MentionSuggestions = nil
		m.suggest.MentionSuggestIndex = 0
		m.suggest.MentionActiveQuery = ""
		m.suggest.MentionFilterQuery = ""
		m.suggest.MentionUserSelected = false
		return m, nil
	}

	var cmd tea.Cmd
	if m.suggest.MentionIndexDir != m.workDir && !m.suggest.MentionIndexLoading {
		m.suggest.MentionIndexLoading = true
		cmd = loadMentionIndex(m.workDir)
	}

	if len(m.suggest.MentionIndex) > 0 && m.suggest.MentionIndexDir == m.workDir {
		filterQuery := query
		if _, preview := mention.MatchSuggestionIndex(m.suggest.MentionIndex, query); preview {
			filterQuery = m.suggest.MentionFilterQuery
		} else {
			m.suggest.MentionFilterQuery = query
		}

		m.suggest.MentionSuggestions = mention.Suggest(filterQuery, m.suggest.MentionIndex)
		if query != m.suggest.MentionActiveQuery {
			m.suggest.MentionUserSelected = false
			if idx, matched := mention.MatchSuggestionIndex(m.suggest.MentionSuggestions, query); matched {
				m.suggest.MentionSuggestIndex = idx
			} else {
				m.suggest.MentionSuggestIndex = 0
			}
		}
		if m.suggest.MentionSuggestIndex >= len(m.suggest.MentionSuggestions) {
			m.suggest.MentionSuggestIndex = 0
		}
		m.suggest.MentionActiveQuery = query
	} else {
		m.suggest.MentionSuggestions = nil
	}
	return m, cmd
}

func (m Model) applyMentionPreview() Model {
	if len(m.suggest.MentionSuggestions) == 0 {
		return m
	}

	_, start, ok := m.activeMention()
	if !ok {
		return m
	}

	selected := m.suggest.MentionSuggestions[m.suggest.MentionSuggestIndex]
	cursor := m.inputCursorOffset()
	m.input.SetValue(mention.Complete(m.input.Value(), start, cursor, selected))
	m = m.syncPromptPrefix()
	m = m.syncInputWidth()
	return m
}

func (m Model) confirmMention() Model {
	if len(m.suggest.MentionSuggestions) == 0 {
		return m
	}

	_, start, ok := m.activeMention()
	if !ok {
		return m
	}

	selected := m.suggest.MentionSuggestions[m.suggest.MentionSuggestIndex]
	cursor := m.inputCursorOffset()
	completed := mention.Complete(m.input.Value(), start, cursor, selected)
	if !strings.HasSuffix(completed, " ") {
		completed += " "
	}

	m.input.SetValue(completed)
	m.suggest.MentionSuggestions = nil
	m.suggest.MentionSuggestIndex = 0
	m.suggest.MentionActiveQuery = ""
	m.suggest.MentionFilterQuery = ""
	m.suggest.MentionUserSelected = false
	m = m.syncPromptPrefix()
	m = m.syncInputWidth()
	return m
}

func (m Model) shouldConfirmMention() bool {
	if len(m.suggest.MentionSuggestions) == 0 {
		return false
	}
	query, _, ok := m.activeMention()
	if !ok {
		return false
	}
	if m.suggest.MentionUserSelected {
		return true
	}
	_, matched := mention.MatchSuggestionIndex(m.suggest.MentionSuggestions, query)
	return matched
}

func (m Model) mentionTab(delta int) Model {
	if m.shouldConfirmMention() {
		return m.confirmMention()
	}
	m = m.cycleMentionSelection(delta)
	if m.shouldConfirmMention() {
		return m.confirmMention()
	}
	return m
}

func (m Model) moveMentionSelection(delta int) Model {
	if len(m.suggest.MentionSuggestions) == 0 {
		return m
	}
	n := len(m.suggest.MentionSuggestions)
	m.suggest.MentionSuggestIndex = (m.suggest.MentionSuggestIndex + delta%n + n) % n
	m.suggest.MentionUserSelected = true
	return m
}

func (m Model) cycleMentionSelection(delta int) Model {
	if len(m.suggest.MentionSuggestions) == 0 {
		return m
	}

	query, _, ok := m.activeMention()
	if !ok {
		return m
	}
	_, preview := mention.MatchSuggestionIndex(m.suggest.MentionSuggestions, query)
	if strings.TrimSpace(query) == "" || !preview {
		m = m.applyMentionPreview()
		if q, _, ok := m.activeMention(); ok {
			m.suggest.MentionActiveQuery = q
		}
		return m
	}

	n := len(m.suggest.MentionSuggestions)
	m.suggest.MentionSuggestIndex = (m.suggest.MentionSuggestIndex + delta%n + n) % n
	m = m.applyMentionPreview()
	if q, _, ok := m.activeMention(); ok {
		m.suggest.MentionActiveQuery = q
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
		return m.mentionTab(1), true
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

	nameColW := mention.NameColumnWidth(m.suggest.MentionSuggestions)
	rows := make([]paletteRow, len(m.suggest.MentionSuggestions))
	for i, entry := range m.suggest.MentionSuggestions {
		name, _, summary := mention.AlignedRow(entry, nameColW)
		rows[i] = paletteRow{name: name, summary: summary}
	}
	return m.renderPaletteRows(rows, m.suggest.MentionSuggestIndex, nameColW)
}
