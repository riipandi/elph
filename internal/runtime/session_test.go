package runtime

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNewSessionHasID(t *testing.T) {
	s := NewSession(t.TempDir())
	require.NotEmpty(t, s.ID.String())
}

func TestNewSessionBuildsSystemPrompt(t *testing.T) {
	s := NewSession(t.TempDir())
	require.Contains(t, s.SystemPrompt, "You are an expert coding assistant.")
	require.Contains(t, s.SystemPrompt, "## Available Tools")
}

func TestSessionRunTurnReturnsCommand(t *testing.T) {
	s := NewSession(t.TempDir())
	require.NotNil(t, s.RunTurn("hello"))
}