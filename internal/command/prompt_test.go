package command

import (
	"testing"

	"github.com/riipandi/elph/internal/prompt/template"
	"github.com/stretchr/testify/require"
)

func TestExecutePromptTemplate(t *testing.T) {
	ctx := Context{
		PromptTemplates: []template.Template{{
			Name:    "identify",
			Content: "Identify the codebase focusing on $1.",
		}},
	}

	result := Execute("/identify auth", ctx)
	require.True(t, result.OK)
	require.Equal(t, "Identify the codebase focusing on auth.", result.AgentPrompt)
	require.Empty(t, result.Output)
}

func TestBuiltinOverridesPromptTemplate(t *testing.T) {
	ctx := Context{
		PromptTemplates: []template.Template{{
			Name:    "help",
			Content: "custom help prompt",
		}},
	}

	result := Execute("/help", ctx)
	require.True(t, result.OK)
	require.Contains(t, result.Output, "/changelog")
	require.Empty(t, result.AgentPrompt)
}

func TestAllIncludesPromptTemplates(t *testing.T) {
	ctx := Context{
		PromptTemplates: []template.Template{{
			Name:        "identify",
			Description: "Identify the codebase",
		}},
	}

	got := All(ctx)
	names := make([]string, len(got))
	for i, cmd := range got {
		names[i] = cmd.Name
	}
	require.Contains(t, names, "identify")
	require.Contains(t, names, "help")
}

func TestSuggestIncludesPromptTemplates(t *testing.T) {
	ctx := Context{
		PromptTemplates: []template.Template{{
			Name:        "identify",
			Description: "Identify the codebase",
		}},
	}

	got := Suggest("ident", ctx)
	require.NotEmpty(t, got)
	require.Equal(t, "identify", got[0].Name)
}
