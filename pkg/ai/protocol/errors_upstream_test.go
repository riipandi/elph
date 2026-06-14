package protocol

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNewUpstreamHTTPErrorOpenCodeRateLimit(t *testing.T) {
	t.Parallel()

	body := []byte(`{"type":"error","error":{"type":"FreeUsageLimitError","message":"Rate limit exceeded. Please try again later."}}`)
	err := NewUpstreamHTTPError(429, body)
	var pe *ProviderError
	require.True(t, errors.As(err, &pe))
	require.Equal(t, 429, pe.StatusCode)
	require.Equal(t, "rate limited", pe.Title)
	require.Contains(t, pe.Message, "FreeUsageLimitError")
	require.Contains(t, pe.Message, "Rate limit exceeded")
}

func TestWrapStreamErrorStall(t *testing.T) {
	t.Parallel()

	err := WrapStreamError(errors.New("stream stalled: no data from provider"))
	var pe *ProviderError
	require.True(t, errors.As(err, &pe))
	require.Equal(t, "stream stalled", pe.Title)
	require.Contains(t, pe.Message, "No data received")
}