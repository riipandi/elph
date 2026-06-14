package tools

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestDiagnosticCatalog(t *testing.T) {
	require.Len(t, Diagnostic(), 3)
	require.Equal(t, []string{
		DiagnosticListTools,
		DiagnosticSystemPrompt,
		DiagnosticOpenLog,
	}, names(Diagnostic()))
}

func TestGetDiagnosticTool(t *testing.T) {
	def, ok := Get(DiagnosticListTools)
	require.True(t, ok)
	require.Equal(t, CategoryDiagnostic, def.Category)
	require.Equal(t, ApprovalAutoAllow, def.DefaultApproval)
	require.Contains(t, def.Description, "available")
}

func TestGetUnknownTool(t *testing.T) {
	_, ok := Get("diagnostic_unknown")
	require.False(t, ok)
}

func TestResolveNameLegacyAliases(t *testing.T) {
	name, ok := ResolveName("diagnostic_list_tools")
	require.True(t, ok)
	require.Equal(t, DiagnosticListTools, name)
}

func names(defs []Definition) []string {
	names := make([]string, len(defs))
	for i, def := range defs {
		names[i] = def.Name
	}
	return names
}
