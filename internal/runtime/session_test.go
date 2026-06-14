package runtime

import (
	"context"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestNewSessionHasID(t *testing.T) {
	s := NewSession(t.TempDir())
	require.NotEmpty(t, s.ID.String())
}

func TestNewSessionBuildsSystemPrompt(t *testing.T) {
	s := NewSession(t.TempDir())
	require.Contains(t, s.SystemPrompt, "You are an expert AI coding assistant, operate in Elph CLI.")
	require.Contains(t, s.SystemPrompt, "## Available Tools")
}

type stubProvider struct{}

func (stubProvider) ID() string { return "stub" }

func (stubProvider) Complete(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	if req.Stream != nil {
		if req.Stream.OnThinking != nil {
			req.Stream.OnThinking("hidden-thought")
		}
		req.Stream.OnContent("stub reply")
	}
	return provider.TurnResult{Content: "stub reply", Thinking: "hidden-thought"}, nil
}

func TestSessionStartTurnStreamsEvents(t *testing.T) {
	s := NewSession(t.TempDir())
	s.Provider = stubProvider{}
	ctx := context.Background()

	var events []agent.Event
	for evt := range s.StartTurn(ctx, "hello", true) {
		events = append(events, evt)
	}

	require.GreaterOrEqual(t, len(events), 3)
	require.Equal(t, agent.ActivityConnecting, events[0].Activity)
	require.Equal(t, agent.ActivityThinking, events[1].Activity)
	require.Equal(t, agent.EventTurnDone, events[len(events)-1].Kind)
	require.Equal(t, "stub reply", events[len(events)-1].Response)
}
