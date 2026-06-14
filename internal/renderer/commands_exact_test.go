package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/prompttemplate"
	"github.com/stretchr/testify/require"
)

func TestCommandPaletteHiddenWhenFullyTyped(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:list-tools")

	m = m.syncSlashSuggestions()
	require.False(t, m.commandPaletteActive())
	require.False(t, m.argPaletteActive())
	require.Empty(t, m.commandPaletteView())
}

func TestCommandPaletteHiddenWhenHelpFullyTyped(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/help")

	m = m.syncSlashSuggestions()
	require.False(t, m.commandPaletteActive())
}

func TestPromptTemplateShowsArgumentHintPlaceholder(t *testing.T) {
	m := testInputModel(t)
	m.promptTemplates = []prompttemplate.Template{{
		Name:         "identify",
		Description:  "Identify the codebase",
		ArgumentHint: "<focus-area>",
	}}

	m.input.SetValue("/identify")
	m = m.syncSlashSuggestions()

	require.False(t, m.commandPaletteActive())
	require.Equal(t, "<focus-area>", m.input.Placeholder)
}

func TestPartialDiagnosticShowsSinglePaletteRow(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:list-tools")
	m = m.syncSlashSuggestions()
	require.False(t, m.commandPaletteActive())

	m.input.SetValue("/diagnostic:list-too")
	m = m.syncSlashSuggestions()
	require.True(t, m.commandPaletteActive())
	require.Len(t, m.suggest.CmdSuggestions, 1)

	view := stripANSI(m.commandPaletteView())
	require.Contains(t, view, "/diagnostic:list-tools")
	require.Contains(t, view, "List all tools available to the agent")
	require.NotContains(t, view, "/diagnostic: list-tools")
}

func TestArgPaletteStillShowsForExactOpenLog(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("/diagnostic:open-log")

	m = m.syncSlashSuggestions()
	require.True(t, m.argPaletteActive())
	require.False(t, m.commandPaletteActive())
	require.Contains(t, m.input.Placeholder, "system")
	require.Contains(t, m.input.Placeholder, "requests")
}
