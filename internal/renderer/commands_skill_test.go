package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/command"
	"github.com/riipandi/elph/internal/prompttemplate"
	"github.com/stretchr/testify/require"
)

func TestSkillSlashHidesPaletteAndPlaceholderWhileTypingPrompt(t *testing.T) {
	m := testInputModel(t)
	m.slashSkills = []command.SlashSkill{{
		Name:        "code-review",
		Description: "Review code changes",
	}}

	m.input.SetValue("/skill:code-review review this module")
	m = m.syncSlashSuggestions()

	require.False(t, m.commandPaletteActive())
	require.False(t, m.argPaletteActive())
	require.Empty(t, m.input.Placeholder)
	require.Empty(t, m.commandPaletteView())
}

func TestPromptTemplateHidesPaletteWhileTypingFreeformArgs(t *testing.T) {
	m := testInputModel(t)
	m.promptTemplates = []prompttemplate.Template{{
		Name:         "identify",
		Description:  "Identify the codebase",
		ArgumentHint: "<focus-area>",
	}}

	m.input.SetValue("/identify authentication layer")
	m = m.syncSlashSuggestions()

	require.False(t, m.commandPaletteActive())
	require.False(t, m.argPaletteActive())
	require.Empty(t, m.input.Placeholder)
}

func TestPromptTemplateStillShowsArgumentHintBeforeArgs(t *testing.T) {
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
