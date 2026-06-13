package agent

import (
	"context"
	"testing"
	"time"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

type stubProvider struct {
	resp  string
	err   error
	delay time.Duration
}

func (s stubProvider) ID() string { return "stub" }

func (s stubProvider) Complete(ctx context.Context, _ provider.TurnRequest) (string, error) {
	if s.delay > 0 {
		timer := time.NewTimer(s.delay)
		defer timer.Stop()
		select {
		case <-timer.C:
		case <-ctx.Done():
			return "", ctx.Err()
		}
	}
	if s.err != nil {
		return "", s.err
	}
	return s.resp, nil
}

func TestRunTurnEmitsPlaceholderPhasesWithoutProvider(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	var events []Event
	for evt := range RunTurn(ctx, TurnOptions{UserPrompt: "hello"}) {
		events = append(events, evt)
	}

	require.NotEmpty(t, events)
	require.Equal(t, EventTurnDone, events[len(events)-1].Kind)
	require.Contains(t, events[len(events)-1].Response, "hello")

	activityCount := 0
	for _, evt := range events {
		if evt.Kind == EventActivity {
			activityCount++
		}
	}
	require.Equal(t, len(TurnPhases)-1, activityCount)
}

func TestRunTurnUsesProvider(t *testing.T) {
	ctx := context.Background()
	var events []Event
	for evt := range RunTurn(ctx, TurnOptions{
		UserPrompt: "hello",
		Provider:   stubProvider{resp: "upstream reply"},
	}) {
		events = append(events, evt)
	}

	require.Len(t, events, 3)
	require.Equal(t, ActivityConnecting, events[0].Activity)
	require.Equal(t, ActivityThinking, events[1].Activity)
	require.Equal(t, EventTurnDone, events[2].Kind)
	require.Equal(t, "upstream reply", events[2].Response)
}

func TestRunTurnShellContextFastPath(t *testing.T) {
	ctx := context.Background()
	var events []Event
	for evt := range RunTurn(ctx, TurnOptions{UserPrompt: "Ran `ls`\n```\nfile\n```"}) {
		events = append(events, evt)
	}

	require.Len(t, events, 1)
	require.Equal(t, EventTurnDone, events[0].Kind)
	require.Empty(t, events[0].Response)
}

func TestRunTurnRespectsContextCancel(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(30 * time.Millisecond)
		cancel()
	}()

	var events []Event
	for evt := range RunTurn(ctx, TurnOptions{
		UserPrompt: "hello",
		Provider:   stubProvider{resp: "late", delay: 2 * time.Second},
	}) {
		events = append(events, evt)
	}

	for _, evt := range events {
		require.NotEqual(t, EventTurnDone, evt.Kind)
	}
}