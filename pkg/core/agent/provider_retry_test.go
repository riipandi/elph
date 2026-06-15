package agent

import (
	"context"
	"errors"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/ai/utils"
	"github.com/stretchr/testify/require"
)

type retryStubProvider struct {
	errs     []error
	results  []provider.TurnResult
	calls    int
	lastReq  provider.TurnRequest
	streamed bool
}

func (s *retryStubProvider) ID() string { return "retry-stub" }

func (s *retryStubProvider) Complete(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	s.lastReq = req
	s.streamed = req.Stream != nil
	idx := s.calls
	s.calls++
	if idx < len(s.errs) {
		return provider.TurnResult{}, s.errs[idx]
	}
	if idx-len(s.errs) < len(s.results) {
		return s.results[idx-len(s.errs)], nil
	}
	return provider.TurnResult{Content: "ok"}, nil
}

func TestCompleteProviderWithRetryOnRetriableError(t *testing.T) {
	t.Parallel()

	stub := &retryStubProvider{
		errs: []error{
			&provider.ProviderError{StatusCode: 429, Message: "slow down"},
		},
	}
	var retries []int
	result, err := completeProviderWithRetry(
		context.Background(),
		nil,
		0,
		stub,
		provider.TurnRequest{UserPrompt: "hi"},
		ProviderRetryConfig{MaxRetries: 2},
		func(attempt int) { retries = append(retries, attempt) },
	)
	require.NoError(t, err)
	require.Equal(t, "ok", result.Content)
	require.Equal(t, 2, stub.calls)
	require.Equal(t, []int{1}, retries)
}

func TestCompleteProviderWithRetryStopsOnNonRetriableError(t *testing.T) {
	t.Parallel()

	stub := &retryStubProvider{
		errs: []error{
			&provider.ProviderError{StatusCode: 400, Message: "bad request"},
		},
	}
	_, err := completeProviderWithRetry(
		context.Background(),
		nil,
		0,
		stub,
		provider.TurnRequest{UserPrompt: "hi"},
		ProviderRetryConfig{MaxRetries: 2},
		nil,
	)
	require.Error(t, err)
	require.Equal(t, 1, stub.calls)
}

func TestCompleteProviderWithRetryDisablesStreamAfterStall(t *testing.T) {
	t.Parallel()

	stub := &retryStubProvider{
		errs: []error{utils.ErrStreamStall},
	}
	stream := &provider.TurnStream{}
	_, err := completeProviderWithRetry(
		context.Background(),
		nil,
		0,
		stub,
		provider.TurnRequest{UserPrompt: "hi", Stream: stream},
		ProviderRetryConfig{MaxRetries: 1},
		nil,
	)
	require.NoError(t, err)
	require.Equal(t, 2, stub.calls)
	require.Nil(t, stub.lastReq.Stream)
}

func TestCompleteProviderWithRetryRespectsZeroMaxRetries(t *testing.T) {
	t.Parallel()

	stub := &retryStubProvider{
		errs: []error{errors.New("boom")},
	}
	_, err := completeProviderWithRetry(
		context.Background(),
		nil,
		0,
		stub,
		provider.TurnRequest{UserPrompt: "hi"},
		ProviderRetryConfig{MaxRetries: 0},
		nil,
	)
	require.Error(t, err)
	require.Equal(t, 1, stub.calls)
}
