package command

import (
	"testing"

	"github.com/riipandi/elph/internal/prompt/template"
	"github.com/stretchr/testify/require"
)

func TestSuggestDiagnosticColonPrefix(t *testing.T) {
	got := Suggest("diagnostic:", Context{})
	for _, cmd := range got {
		t.Logf("%s | %q", PaletteID(cmd), cmd.Name)
	}
	require.NotEmpty(t, got)
}

func TestSuggestExactDiagnosticListToolsSingleMatch(t *testing.T) {
	got := Suggest("diagnostic:list-tools", Context{})
	require.Len(t, got, 1)
	require.Equal(t, DiagnosticListTools, got[0].Name)
}

func TestSuggestVisibleHidesExactCommand(t *testing.T) {
	require.Empty(t, SuggestVisible("/diagnostic:list-tools", Context{}))
	require.Empty(t, SuggestVisible("/help", Context{}))
	require.NotEmpty(t, SuggestVisible("/diagnostic:list-too", Context{}))
}

func TestCommandExactMatch(t *testing.T) {
	require.True(t, CommandExactMatch("/diagnostic:list-tools", Context{}))
	require.True(t, CommandExactMatch("/help", Context{}))
	require.False(t, CommandExactMatch("/help ", Context{}))
	require.False(t, CommandExactMatch("/diagnostic:list-too", Context{}))
	require.True(t, CommandExactMatch("/diagnostic:open-log", Context{}))
}

func TestInputPlaceholderHint(t *testing.T) {
	ctx := Context{
		PromptTemplates: []template.Template{{
			Name:         "identify",
			Description:  "Identify the codebase",
			ArgumentHint: "<focus-area>",
		}},
	}
	cmd, ok := Get("identify", ctx)
	require.True(t, ok)
	require.Equal(t, "<focus-area>", InputPlaceholderHint(cmd, ctx))
}
