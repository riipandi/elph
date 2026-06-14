package command

import (
	"testing"

	"github.com/riipandi/elph/internal/prompttemplate"
	"github.com/stretchr/testify/require"
)

func TestRequiresArgs(t *testing.T) {
	ctx := Context{
		PromptTemplates: []prompttemplate.Template{{
			Name:         "identify",
			ArgumentHint: "<focus-area>",
		}},
	}

	openLog, ok := Get(DiagnosticOpenLog, Context{})
	require.True(t, ok)
	require.True(t, RequiresArgs(openLog, Context{}))

	identify, ok := Get("identify", ctx)
	require.True(t, ok)
	require.True(t, RequiresArgs(identify, ctx))

	help, ok := Get("help", Context{})
	require.True(t, ok)
	require.False(t, RequiresArgs(help, Context{}))
}

func TestSkillSlashWithoutArgumentHintIsOptional(t *testing.T) {
	ctx := Context{
		Skills: []SlashSkill{{
			Name:        "code-review",
			Description: "Review code changes",
		}},
	}
	cmd, ok := Get("skill:code-review", ctx)
	require.True(t, ok)
	require.True(t, cmd.Skill)
	require.False(t, RequiresArgs(cmd, ctx))
	require.Empty(t, InputPlaceholderHint(cmd, ctx))
}

func TestSkillSlashUsesArgumentHintLikePromptTemplate(t *testing.T) {
	ctx := Context{
		Skills: []SlashSkill{{
			Name:         "identify",
			Description:  "Identify the codebase",
			ArgumentHint: "<focus-area>",
		}},
	}
	cmd, ok := Get("skill:identify", ctx)
	require.True(t, ok)
	require.True(t, RequiresArgs(cmd, ctx))
	require.Equal(t, "<focus-area>", InputPlaceholderHint(cmd, ctx))
	require.Equal(t, "/skill:identify ", CompleteInput(cmd, ctx))
}

func TestCompleteInputAddsSpaceForArgumentHint(t *testing.T) {
	ctx := Context{
		PromptTemplates: []prompttemplate.Template{{
			Name:         "identify",
			ArgumentHint: "<focus-area>",
		}},
	}
	cmd, ok := Get("identify", ctx)
	require.True(t, ok)
	require.Equal(t, "/identify ", CompleteInput(cmd, ctx))
}
