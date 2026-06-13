package runtime

import (
	"context"
	"testing"
	"time"

	"github.com/riipandi/elph/pkg/core/agent"
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

func TestSessionStartTurnStreamsEvents(t *testing.T) {
	s := NewSession(t.TempDir())
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	var events []agent.Event
	for evt := range s.StartTurn(ctx, "hello") { // placeholder when no API key
		events = append(events, evt)
	}

	require.NotEmpty(t, events)
	require.Equal(t, agent.EventTurnDone, events[len(events)-1].Kind)
}