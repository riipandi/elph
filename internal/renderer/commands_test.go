package renderer

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/command"
	"github.com/stretchr/testify/require"
)

func TestCommandPaletteAppearsForSlashInput(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:")

	m = m.syncSlashSuggestions()
	require.True(t, m.commandPaletteActive())
	require.NotEmpty(t, m.commandPaletteView())
	for _, cmd := range m.suggest.CmdSuggestions {
		require.Contains(t, cmd.Name, "diagnostic:")
	}
}

func TestCommandPaletteHiddenForNormalInput(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	m = m.syncSlashSuggestions()
	require.False(t, m.commandPaletteActive())
}

func TestCommandPaletteTwoColumnLayout(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/")
	m = m.syncSlashSuggestions()
	require.GreaterOrEqual(t, len(m.suggest.CmdSuggestions), 2)

	got := command.FormatList(m.suggest.CmdSuggestions)
	lines := strings.Split(got, "\n")
	require.GreaterOrEqual(t, len(lines), 2)

	firstSummary := strings.Index(lines[0], m.suggest.CmdSuggestions[0].Description)
	secondSummary := strings.Index(lines[1], m.suggest.CmdSuggestions[1].Description)
	require.Equal(t, firstSummary, secondSummary)

	view := stripANSI(m.commandPaletteView())
	require.Contains(t, view, "/"+m.suggest.CmdSuggestions[0].Name)
	require.Contains(t, view, m.suggest.CmdSuggestions[0].Description)
}

func TestTabCompletesSelectedCommand(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/help")
	m = m.syncSlashSuggestions()

	updated, consumed := m.handleSlashPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "/help", updated.input.Value())
}

func TestDownCyclesSuggestionSelection(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/")
	m = m.syncSlashSuggestions()
	require.GreaterOrEqual(t, len(m.suggest.CmdSuggestions), 2)

	updated, consumed := m.handleSlashPaletteKey(keyDown())
	require.True(t, consumed)
	require.Equal(t, 1, updated.suggest.CmdSuggestIndex)
}

func TestPaletteSitsFlushAboveInput(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/help")
	m = m.syncSlashSuggestions()

	chromeH := lipgloss.Height(m.inputChromeView())
	paletteH := lipgloss.Height(m.commandPaletteView())
	inputH := lipgloss.Height(m.inputBoxView(true))
	require.Equal(t, paletteH+inputH, chromeH)
}

func TestCompleteInputUsesCatalogName(t *testing.T) {
	cmd, ok := command.Get(command.DiagnosticListTools, command.Context{})
	require.True(t, ok)
	require.Equal(t, "/diagnostic:list-tools", command.CompleteInput(cmd, command.Context{}))
}

func TestArgPaletteAppearsForOpenLog(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log")

	m = m.syncSlashSuggestions()
	require.True(t, m.argPaletteActive())
	require.False(t, m.commandPaletteActive())
	require.Len(t, m.suggest.ArgSuggestions, 2)
}

func TestOpenLogPlaceholderShowsArgHint(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log ")

	m = m.syncSlashSuggestions()
	require.Equal(t, "requests | system", m.input.Placeholder)
}

func TestTabCyclesArgSelection(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log ")
	m = m.syncSlashSuggestions()

	updated, consumed := m.handleSlashPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "/diagnostic:open-log requests", updated.input.Value())

	updated, consumed = updated.handleSlashPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "/diagnostic:open-log system", updated.input.Value())

	updated, consumed = updated.handleSlashPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "/diagnostic:open-log requests", updated.input.Value())
}

func TestArgPaletteFiltersByQuery(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log sys")
	m = m.syncSlashSuggestions()

	require.True(t, m.argPaletteActive())
	require.Len(t, m.suggest.ArgSuggestions, 1)
	require.Equal(t, "system", m.suggest.ArgSuggestions[0].Value)
}

func TestShiftTabCyclesArgSelectionBackward(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log requests")
	m = m.syncSlashSuggestions()

	updated, consumed := m.handleSlashPaletteKey(keyShiftTab())
	require.True(t, consumed)
	require.Equal(t, "/diagnostic:open-log system", updated.input.Value())
}
