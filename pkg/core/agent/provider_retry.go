package agent

import (
	"context"
	"errors"
	"strings"
	"time"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/ai/utils"
)

const providerRetryBackoff = time.Second

// ProviderRetryConfig controls provider request retries and stream timeouts.
type ProviderRetryConfig struct {
	MaxRetries         int
	StreamStallTimeout time.Duration
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
	var pe *provider.ProviderError
	if errors.As(err, &pe) && pe != nil {
		if pe.IsRetriable() {
			return true
		}
	}
	return provider.ShouldStreamNonStreamingFallback(err)
}

func shouldDisableStreamOnRetry(err error) bool {
	if errors.Is(err, utils.ErrStreamStall) {
		return true
	}
	return provider.ShouldStreamNonStreamingFallback(err)
}

func completeProviderWithRetry(
	ctx context.Context,
	log TurnLogFunc,
	step int,
	p provider.Provider,
	req provider.TurnRequest,
	cfg ProviderRetryConfig,
	onRetry func(attempt int),
) (provider.TurnResult, error) {
	var lastErr error
	for attempt := 0; attempt < cfg.attempts(); attempt++ {
		if attempt > 0 {
			if !shouldRetryProvider(lastErr) {
				break
			}
			logProviderRetry(log, step, attempt, lastErr)
			if onRetry != nil {
				onRetry(attempt)
			}
			if !wait(ctx, providerRetryBackoff*time.Duration(attempt)) {
				return provider.TurnResult{}, ctx.Err()
			}
		}

		attemptReq := req
		attemptReq.StreamStallTimeout = utils.EffectiveStreamStallTimeout(cfg.StreamStallTimeout)
		if attempt > 0 && shouldDisableStreamOnRetry(lastErr) {
			attemptReq.Stream = nil
		}

		result, err := p.Complete(ctx, attemptReq)
		if err == nil {
			return result, nil
		}
		lastErr = err
		if ctx.Err() != nil || ProviderCancelError(err) || !shouldRetryProvider(err) {
			return provider.TurnResult{}, err
		}
		if attempt+1 >= cfg.attempts() {
			return provider.TurnResult{}, err
		}
	}
	if lastErr != nil {
		return provider.TurnResult{}, lastErr
	}
	return provider.TurnResult{}, errors.New("provider: retry failed")
}
