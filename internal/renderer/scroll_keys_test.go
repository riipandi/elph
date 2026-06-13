package renderer

import (
	"fmt"
	"testing"

	"github.com/charmbracelet/bubbletea"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func scrollableTestModel(t *testing.T) Model {
	t.Helper()
	m := New()
	m.width = 80
	m.height = 14
	m.ready = true
	for i := range 30 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("scroll test line %d", i),
			kind: constants.MessageUser,
		})
	}
	m.contentDirty = true
	return m.syncLayout(false)
}

func TestPlainArrowDoesNotScrollContent(t *testing.T) {
	m := scrollableTestModel(t)
	m.input.Focus()
	offset := m.content.YOffset

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m = updated.(Model)

	require.Equal(t, offset, m.content.YOffset)
}

func TestPlainJKHLDoesNotScrollContent(t *testing.T) {
	m := scrollableTestModel(t)
	m.input.Focus()
	offset := m.content.YOffset

	for _, r := range []rune{'j', 'k', 'h', 'l'} {
		updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}})
		m = updated.(Model)
	}

	require.Equal(t, offset, m.content.YOffset)
	require.Equal(t, "jkhl", m.input.Value())
}

func TestShiftArrowScrollsContent(t *testing.T) {
	m := scrollableTestModel(t)
	m.content.GotoTop()
	m.input.Focus()
	m.input.SetValue("typed")
	offset := m.content.YOffset

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyShiftDown})
	m = updated.(Model)

	require.Greater(t, m.content.YOffset, offset)
	require.Equal(t, "typed", m.input.Value())
}

func TestShiftArrowScrollsUp(t *testing.T) {
	m := scrollableTestModel(t)
	m.content.GotoBottom()
	m.input.Focus()
	offset := m.content.YOffset

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyShiftUp})
	m = updated.(Model)

	require.Less(t, m.content.YOffset, offset)
}