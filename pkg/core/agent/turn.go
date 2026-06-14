package agent

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/riipandi/elph/pkg/ai/provider"
)

const PhaseDelay = 400 * time.Millisecond

// IsShellContextPrompt reports Pi-style shell output queued for the agent (!cmd).
func IsShellContextPrompt(prompt string) bool {
	return strings.HasPrefix(strings.TrimSpace(prompt), "Ran `")
}

// RunTurn executes an agent turn and streams framework-neutral events.
// The channel is closed after the turn completes or ctx is cancelled.
// When opts.Provider is nil, a local placeholder simulation is used.
func RunTurn(ctx context.Context, opts TurnOptions) <-chan Event {
	ch := make(chan Event, len(TurnPhases)+2)
	go runTurn(ctx, opts, ch)
	return ch
}

func runTurn(ctx context.Context, opts TurnOptions, ch chan<- Event) {
	defer close(ch)

	if IsShellContextPrompt(opts.UserPrompt) {
		sendEvent(ctx, ch, TurnDoneEvent(provider.TurnResult{Content: PlaceholderResponse(opts.UserPrompt)}))
		return
	}

	if opts.Provider == nil {
		runPlaceholderTurn(ctx, opts.UserPrompt, ch)
		return
	}

	if opts.ToolsEnabled && opts.ExecuteTool != nil {
		runProviderLoop(ctx, opts, ch)
		return
	}

	if !sendEvent(ctx, ch, ActivityEvent(ActivityConnecting)) {
		return
	}
	if !sendEvent(ctx, ch, ActivityEvent(ActivityThinking)) {
		return
	}

	stream := &provider.TurnStream{
		OnContent: func(chunk string) {
			sendEvent(ctx, ch, ResponseDeltaEvent(chunk))
		},
	}
	if opts.ShowThinking {
		stream.OnThinking = func(chunk string) {
			sendEvent(ctx, ch, ThinkingDeltaEvent(chunk))
		}
	}

	result, err := opts.Provider.Complete(ctx, provider.TurnRequest{
		SystemPrompt: opts.SystemPrompt,
		UserPrompt:   opts.UserPrompt,
		Model:        opts.Model,
		Thinking:     opts.Thinking,
		Compat:       opts.Compat,
		Stream:       stream,
		Messages:     opts.Messages,
	})
	if ctx.Err() != nil {
		return
	}
	if err != nil {
		sendEvent(ctx, ch, TurnDoneEvent(provider.TurnResult{Content: fmt.Sprintf("Provider error: %v", err)}))
		return
	}
	if !opts.ShowThinking {
		result.Thinking = ""
	}
	sendEvent(ctx, ch, TurnDoneEvent(result))
}

func runPlaceholderTurn(ctx context.Context, prompt string, ch chan<- Event) {
	start := time.Now()
	for i, phase := range TurnPhases[1:] {
		if !waitUntil(ctx, start, PhaseDelay*time.Duration(i+1)) {
			return
		}
		if !sendEvent(ctx, ch, ActivityEvent(phase)) {
			return
		}
	}

	if !waitUntil(ctx, start, PhaseDelay*time.Duration(len(TurnPhases))) {
		return
	}
	sendEvent(ctx, ch, TurnDoneEvent(provider.TurnResult{Content: PlaceholderResponse(prompt)}))
}

func waitUntil(ctx context.Context, start time.Time, target time.Duration) bool {
	remaining := target - time.Since(start)
	if remaining <= 0 {
		return true
	}
	return wait(ctx, remaining)
}

func wait(ctx context.Context, d time.Duration) bool {
	timer := time.NewTimer(d)
	defer timer.Stop()
	select {
	case <-timer.C:
		return true
	case <-ctx.Done():
		return false
	}
}

// PlaceholderResponse is a stub assistant reply used when no provider is configured.
func PlaceholderResponse(prompt string) string {
	if IsShellContextPrompt(prompt) {
		return ""
	}
	return fmt.Sprintf("Received: %s\n\n(Agent integration pending — this is a placeholder response.)", prompt)
}

func sendEvent(ctx context.Context, ch chan<- Event, evt Event) bool {
	select {
	case ch <- evt:
		return true
	case <-ctx.Done():
		return false
	}
}
