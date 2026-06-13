package renderer

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/riipandi/elph/internal/mention"
	"github.com/stretchr/testify/require"
)

func seedMentionIndex(m Model, entries []mention.Entry) Model {
	m.suggest.MentionIndex = entries
	m.suggest.MentionIndexDir = m.workDir
	return m
}

func TestMentionPaletteAppearsForAtQuery(t *testing.T) {
	m := testInputModel(t)
	m = seedMentionIndex(m, []mention.Entry{
		{Path: "internal/renderer/input.go"},
		{Path: "internal", IsDir: true},
	})
	m.input.SetValue("fix @input")

	m = m.syncSlashSuggestions()
	require.True(t, m.mentionPaletteActive())
	require.NotEmpty(t, m.commandPaletteView())
	require.Contains(t, m.commandPaletteView(), "internal/renderer/input.go")
}

func TestMentionPaletteHiddenForSlashInput(t *testing.T) {
	m := testInputModel(t)
	m = seedMentionIndex(m, []mention.Entry{{Path: "internal/renderer/input.go"}})
	m.input.SetValue("/help")

	m = m.syncSlashSuggestions()
	require.False(t, m.mentionPaletteActive())
	require.True(t, m.commandPaletteActive())
}

func TestTabCyclesMentionPreview(t *testing.T) {
	m := testInputModel(t)
	m = seedMentionIndex(m, []mention.Entry{
		{Path: "internal/renderer/input.go"},
		{Path: "internal/command/args.go"},
	})
	m.input.SetValue("see @")
	m = m.syncSlashSuggestions()

	updated, consumed := m.handleInputPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "see @internal/renderer/input.go", updated.input.Value())
	require.Equal(t, 0, updated.suggest.MentionSuggestIndex)

	updated, consumed = updated.handleInputPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "see @internal/command/args.go", updated.input.Value())
	require.Equal(t, 1, updated.suggest.MentionSuggestIndex)

	updated = updated.syncSlashSuggestions()
	require.Equal(t, 1, updated.suggest.MentionSuggestIndex)

	updated, consumed = updated.handleInputPaletteKey(keyShiftTab())
	require.True(t, consumed)
	require.Equal(t, "see @internal/renderer/input.go", updated.input.Value())
}

func TestTabAppliesFirstMatchForPartialQuery(t *testing.T) {
	m := testInputModel(t)
	m = seedMentionIndex(m, []mention.Entry{
		{Path: "internal/renderer/input.go"},
		{Path: "internal", IsDir: true},
	})
	m.input.SetValue("see @input")
	m = m.syncSlashSuggestions()

	updated, consumed := m.handleInputPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "see @internal/renderer/input.go", updated.input.Value())
}

func TestEnterConfirmsMentionWithoutSubmit(t *testing.T) {
	m := testInputModel(t)
	m = seedMentionIndex(m, []mention.Entry{
		{Path: "internal/renderer/input.go"},
		{Path: "internal", IsDir: true},
	})
	m.input.SetValue("see @input")
	m = m.syncSlashSuggestions()

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.Nil(t, cmd)
	require.False(t, m.agent.Busy)
	require.Empty(t, m.messages)
	require.Equal(t, "see @internal/renderer/input.go ", m.input.Value())
	require.False(t, m.mentionPaletteActive())
}

func TestEnterConfirmsHighlightedMention(t *testing.T) {
	m := testInputModel(t)
	m = seedMentionIndex(m, []mention.Entry{
		{Path: "internal/renderer/input.go"},
		{Path: "internal/command/args.go"},
	})
	m.input.SetValue("see @")
	m = m.syncSlashSuggestions()

	updated, _ := m.handleInputPaletteKey(keyDown())
	finished, cmd := updated.Update(keyEnter())
	m = finished.(Model)

	require.Nil(t, cmd)
	require.Equal(t, "see @internal/command/args.go ", m.input.Value())
}

func TestMentionIndexLoadsFromWorkDir(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.MkdirAll(filepath.Join(dir, "pkg", "app"), 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, "pkg", "app", "main.go"), []byte("ok"), 0o644))

	m := testInputModel(t)
	m.workDir = dir
	m.input.SetValue("use @pkg")

	updated, cmd := m.syncInputSuggestions()
	require.NotNil(t, cmd)

	loaded, _ := updated.Update(mentionIndexMsg{
		workDir: dir,
		entries: []mention.Entry{{Path: "pkg", IsDir: true}, {Path: "pkg/app", IsDir: true}, {Path: "pkg/app/main.go"}},
	})
	m = loaded.(Model)
	require.True(t, m.mentionPaletteActive())
}
