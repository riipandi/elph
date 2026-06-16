package agent

import (
	"context"
	"errors"
	"strings"
	"time"

	"github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/riipandi/elph/pkg/ai/utils"
)

const providerRetryBackoff = time.Second

// maxContextCompactions is the max times we compact and retry after
// a context-too-large error before giving up.
const maxContextCompactions = 3

// ProviderRetryConfig controls provider request retries and stream timeouts.
type ProviderRetryConfig struct {
	MaxRetries         int
	StreamStallTimeout time.Duration
	AutoCompactContext bool // when true, compact messages on context-limit and retry
	AutoCompactLimit   int  // compaction target percentage (0 = use default 80)
}

func (cfg ProviderRetryConfig) attempts() int {
	if cfg.MaxRetries < 0 {
		return 1
	}
	return cfg.MaxRetries + 1
}

func shouldRetryProvider(err error) bool {
	if err == nil || ProviderCancelError(err) {
		return false
	}
	if errors.Is(err, utils.ErrStreamStall) {
		return true
	}
	if msg := strings.ToLower(err.Error()); strings.Contains(msg, "stream stalled") || strings.Contains(msg, "idle timeout") {
		return true
	}
	var pe *protocol.ProviderError
	if errors.As(err, &pe) && pe != nil {
		if pe.IsContextTooLarge() {
			return true
		}
		if pe.IsRetriable() {
			return true
		}
	}
	return protocol.ShouldStreamNonStreamingFallback(err)
}

func shouldDisableStreamOnRetry(err error) bool {
	if errors.Is(err, utils.ErrStreamStall) {
		return true
	}
	return protocol.ShouldStreamNonStreamingFallback(err)
}

// isContextTooLargeProviderError reports whether an error represents a
// context-window overflow from the upstream provider.
func isContextTooLargeProviderError(err error) bool {
	var pe *protocol.ProviderError
	if errors.As(err, &pe) && pe != nil {
		return pe.IsContextTooLarge()
	}
	return false
}

func completeProviderWithRetry(
	ctx context.Context,
	log TurnLogFunc,
	step int,
	p protocol.Provider,
	req protocol.TurnRequest,
	cfg ProviderRetryConfig,
	onRetry func(attempt int),
	onCompaction func(result CompactionResult),
) (protocol.TurnResult, error) {
	var lastErr error
	baseAttempts := cfg.attempts()
	totalBudget := baseAttempts
	if cfg.AutoCompactContext {
		totalBudget += maxContextCompactions
	}
	contextCompactions := 0

	for attempt := 0; attempt < totalBudget; attempt++ {
		if attempt > 0 {
			// Context-too-large: compact and retry without backoff.
			if cfg.AutoCompactContext && contextCompactions < maxContextCompactions && isContextTooLargeProviderError(lastErr) {
				tokensBefore := EstimateTokens(historyUTF8Size(req.Messages))
				result := CompactMessagesWithEntry(req.Messages, cfg.AutoCompactLimit, ReasonOverflow, tokensBefore)
				if result.Changed {
					req.Messages = result.Messages
					contextCompactions++
					logProviderRetry(log, step, attempt, lastErr)
					if onRetry != nil {
						onRetry(attempt)
					}
					if onCompaction != nil {
						onCompaction(result)
					}
					// Skip the backoff — retry immediately with compacted history.
					attemptReq := req
					attemptReq.StreamStallTimeout = utils.EffectiveStreamStallTimeout(cfg.StreamStallTimeout)
					resp, err := p.Complete(ctx, attemptReq)
					if err == nil {
						return resp, nil
					}
					lastErr = err
					if ctx.Err() != nil || ProviderCancelError(err) {
						return protocol.TurnResult{}, err
					}
					continue
				}
				// Compaction achieved nothing — fall through.
			}

			// Standard retry with provider backoff
			if !shouldRetryProvider(lastErr) {
				break
			}
			logProviderRetry(log, step, attempt, lastErr)
			if onRetry != nil {
				onRetry(attempt)
			}
			if !wait(ctx, providerRetryBackoff*time.Duration(attempt)) {
				return protocol.TurnResult{}, ctx.Err()
			}
		}

		attemptReq := req
		attemptReq.StreamStallTimeout = utils.EffectiveStreamStallTimeout(cfg.StreamStallTimeout)
		if attempt > 0 && shouldDisableStreamOnRetry(lastErr) {
			attemptReq.Stream = nil
		}

		resp, err := p.Complete(ctx, attemptReq)
		if err == nil {
			return resp, nil
		}
		lastErr = err
		if ctx.Err() != nil || ProviderCancelError(err) || !shouldRetryProvider(err) {
			return protocol.TurnResult{}, err
		}
		if attempt+1 >= totalBudget {
			return protocol.TurnResult{}, err
		}
	}
	if lastErr != nil {
		return protocol.TurnResult{}, lastErr
	}
	return protocol.TurnResult{}, errors.New("provider: retry failed")
}
