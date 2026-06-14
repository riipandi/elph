package command

import (
	"strings"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFormatListTwoColumnLayout(t *testing.T) {
	got := FormatList([]SlashCommand{
		{Name: "help", Description: "Show available slash commands"},
		{Name: DiagnosticListTools, Description: "List all tools available to the agent"},
	})

	lines := strings.Split(got, "\n")
	require.Len(t, lines, 2)
	require.True(t, strings.HasPrefix(lines[0], "  /help"))
	require.Contains(t, lines[0], "Show available slash commands")
	require.True(t, strings.HasPrefix(lines[1], "  /"+DiagnosticListTools))

	helpIdx := strings.Index(lines[0], "Show available")
	toolsIdx := strings.Index(lines[1], "List all tools")
	require.Equal(t, helpIdx, toolsIdx)
}

func TestFormatHelpIncludesHeaderAndAliases(t *testing.T) {
	got := FormatHelp(All(Context{}))
	require.Contains(t, got, "Available slash commands:")
	require.Contains(t, got, "/exit (quit, q)")
}
