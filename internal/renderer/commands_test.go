package renderer

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/command"
	"github.com/riipandi/elph/internal/prompttemplate"
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
	m.input.SetValue("/hel")
	m = m.syncSlashSuggestions()

	updated, _, consumed := m.handleSlashPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "/help", updated.input.Value())
}

func TestDownCyclesSuggestionSelection(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/")
	m = m.syncSlashSuggestions()
	require.GreaterOrEqual(t, len(m.suggest.CmdSuggestions), 2)

	updated, _, consumed := m.handleSlashPaletteKey(keyDown())
	require.True(t, consumed)
	require.Equal(t, 1, updated.suggest.CmdSuggestIndex)
}

func TestPaletteSitsFlushAboveInput(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/hel")
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
	require.Len(t, m.suggest.ArgSuggestions, 5)
}

func TestOpenLogPlaceholderShowsArgHint(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log ")

	m = m.syncSlashSuggestions()
	require.Contains(t, m.input.Placeholder, "system")
	require.Contains(t, m.input.Placeholder, "thinking")
	require.Contains(t, m.input.Placeholder, "requests")
}

func TestTabCyclesArgSelection(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log ")
	m = m.syncSlashSuggestions()

	updated, _, consumed := m.handleSlashPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "/diagnostic:open-log system", updated.input.Value())

	updated, _, consumed = updated.handleSlashPaletteKey(keyTab())
	require.True(t, consumed)
	require.Equal(t, "/diagnostic:open-log thinking", updated.input.Value())
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

	updated, _, consumed := m.handleSlashPaletteKey(keyShiftTab())
	require.True(t, consumed)
	require.Equal(t, "/diagnostic:open-log ai", updated.input.Value())
}

func TestEnterExecutesSelectedCommandWithoutArgs(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/hel")
	m = m.syncSlashSuggestions()

	updated, cmd, consumed := m.handleSlashPaletteKey(keyEnter())
	require.True(t, consumed)
	require.Nil(t, cmd)
	require.Empty(t, updated.input.Value())
	require.Len(t, updated.messages, 2)
	require.Equal(t, "/help", updated.messages[0].text)
	require.Contains(t, updated.messages[1].text, "/changelog")
}

func TestEnterCompletesCommandWithRequiredArgs(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-lo")
	m = m.syncSlashSuggestions()

	updated, cmd, consumed := m.handleSlashPaletteKey(keyEnter())
	require.True(t, consumed)
	require.Nil(t, cmd)
	require.Equal(t, "/diagnostic:open-log ", updated.input.Value())
	require.True(t, updated.argPaletteActive())
	require.Empty(t, updated.messages)
}

func TestEnterExecutesSelectedArg(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log")
	m = m.syncSlashSuggestions()

	updated, cmd, consumed := m.handleSlashPaletteKey(keyEnter())
	require.True(t, consumed)
	require.Nil(t, cmd)
	require.Empty(t, updated.input.Value())
	require.Len(t, updated.messages, 2)
	require.Equal(t, "/diagnostic:open-log system", updated.messages[0].text)
	require.Contains(t, updated.messages[1].text, ".agents/elph/logs/")
}

func TestEnterOnPromptTemplateCompletesWithoutExecuting(t *testing.T) {
	m := testInputModel(t)
	m.promptTemplates = []prompttemplate.Template{{
		Name:         "identify",
		Description:  "Identify the codebase",
		ArgumentHint: "<focus-area>",
	}}
	m.input.SetValue("/ident")
	m = m.syncSlashSuggestions()

	updated, cmd, consumed := m.handleSlashPaletteKey(keyEnter())
	require.True(t, consumed)
	require.Nil(t, cmd)
	require.Equal(t, "/identify ", updated.input.Value())
	require.Equal(t, "<focus-area>", updated.input.Placeholder)
	require.False(t, updated.agent.Busy)
}
