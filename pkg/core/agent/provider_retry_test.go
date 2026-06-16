package agent

import (
	"context"
	"errors"
	"testing"

	"github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/riipandi/elph/pkg/ai/utils"
	"github.com/stretchr/testify/require"
)

type retryStubProvider struct {
	errs     []error
	results  []protocol.TurnResult
	calls    int
	lastReq  protocol.TurnRequest
	streamed bool
}

func (s *retryStubProvider) ID() string { return "retry-stub" }

func (s *retryStubProvider) Complete(ctx context.Context, req protocol.TurnRequest) (protocol.TurnResult, error) {
	s.lastReq = req
	s.streamed = req.Stream != nil
	idx := s.calls
	s.calls++
	if idx < len(s.errs) {
		return protocol.TurnResult{}, s.errs[idx]
	}
	if idx-len(s.errs) < len(s.results) {
		return s.results[idx-len(s.errs)], nil
	}
	return protocol.TurnResult{Content: "ok"}, nil
}

func TestCompleteProviderWithRetryOnRetriableError(t *testing.T) {
	t.Parallel()

	stub := &retryStubProvider{
		errs: []error{
			&protocol.ProviderError{StatusCode: 429, Message: "slow down"},
		},
	}
	var retries []int
	result, err := completeProviderWithRetry(
		context.Background(),
		nil,
		0,
		stub,
		protocol.TurnRequest{UserPrompt: "hi"},
		ProviderRetryConfig{MaxRetries: 2},
		func(attempt int) { retries = append(retries, attempt) },
		nil,
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
			&protocol.ProviderError{StatusCode: 400, Message: "bad request"},
		},
	}
	_, err := completeProviderWithRetry(
		context.Background(),
		nil,
		0,
		stub,
		protocol.TurnRequest{UserPrompt: "hi"},
		ProviderRetryConfig{MaxRetries: 2},
		nil,
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
	stream := &protocol.TurnStream{}
	_, err := completeProviderWithRetry(
		context.Background(),
		nil,
		0,
		stub,
		protocol.TurnRequest{UserPrompt: "hi", Stream: stream},
		ProviderRetryConfig{MaxRetries: 1},
		nil,
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
		protocol.TurnRequest{UserPrompt: "hi"},
		ProviderRetryConfig{MaxRetries: 0},
		nil,
		nil,
	)
	require.Error(t, err)
	require.Equal(t, 1, stub.calls)
}
