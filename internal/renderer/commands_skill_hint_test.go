package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/command"
	"github.com/riipandi/elph/internal/prompttemplate"
	"github.com/stretchr/testify/require"
)

func TestSkillSlashShowsArgumentHintPlaceholder(t *testing.T) {
	m := testInputModel(t)
	m.promptTemplates = []prompttemplate.Template{}
	m.slashSkills = []command.SlashSkill{{
		Name:         "identify",
		Description:  "Identify the codebase",
		ArgumentHint: "<focus-area>",
	}}

	m.input.SetValue("/skill:identify")
	m = m.syncSlashSuggestions()

	require.False(t, m.commandPaletteActive())
	require.Equal(t, "<focus-area>", m.input.Placeholder)
}

func TestSkillSlashClearsPlaceholderWhenArgsTyped(t *testing.T) {
	m := testInputModel(t)
	m.promptTemplates = []prompttemplate.Template{}
	m.slashSkills = []command.SlashSkill{{
		Name:         "identify",
		Description:  "Identify the codebase",
		ArgumentHint: "<focus-area>",
	}}

	m.input.SetValue("/skill:identify authentication")
	m = m.syncSlashSuggestions()

	require.False(t, m.commandPaletteActive())
	require.Empty(t, m.input.Placeholder)
}