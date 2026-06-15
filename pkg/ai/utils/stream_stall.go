package utils

import (
	"context"
	"errors"
	"time"
)

// StreamStallTimeout is how long to wait without stream activity before failing.
// Slow-thinking models (e.g. MiMo with qwen format) may take over a minute
// before the first SSE chunk arrives.
const StreamStallTimeout = 120 * time.Second

// EffectiveStreamStallTimeout returns timeout when positive, otherwise the default.
func EffectiveStreamStallTimeout(timeout time.Duration) time.Duration {
	if timeout > 0 {
		return timeout
	}
	return StreamStallTimeout
}

// ErrStreamStall is returned when no SSE chunk arrives within StreamStallTimeout.
var ErrStreamStall = errors.New("stream stalled: no data from provider")

// WithStreamStallWatch returns a child context cancelled when no activity is
// reported for timeout. Call the bump function whenever a chunk is received.
func WithStreamStallWatch(ctx context.Context, timeout time.Duration) (context.Context, func()) {
	if timeout <= 0 {
		return ctx, func() {}
	}

	streamCtx, cancel := context.WithCancelCause(ctx)
	activity := make(chan struct{}, 1)
	activity <- struct{}{}

	go func() {
		timer := time.NewTimer(timeout)
		defer timer.Stop()
		for {
			select {
			case <-streamCtx.Done():
				return
			case <-activity:
				if !timer.Stop() {
					select {
					case <-timer.C:
					default:
					}
				}
				timer.Reset(timeout)
			case <-timer.C:
				cancel(ErrStreamStall)
				return
			}
		}
	}()

	bump := func() {
		select {
		case activity <- struct{}{}:
		default:
		}
	}
	return streamCtx, bump
}
