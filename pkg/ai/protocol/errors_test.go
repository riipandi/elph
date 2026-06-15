package protocol

import (
	"encoding/json"
	"errors"
	"net/http"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestIsStreamJSONError(t *testing.T) {
	t.Parallel()

	require.False(t, IsStreamJSONError(nil))
	require.True(t, IsStreamJSONError(&json.SyntaxError{}))
	require.True(t, IsStreamJSONError(errors.New("unexpected end of JSON input")))
	require.False(t, IsStreamJSONError(errors.New("connection reset")))
}

func TestShouldStreamNonStreamingFallback(t *testing.T) {
	t.Parallel()

	require.False(t, ShouldStreamNonStreamingFallback(nil))
	require.True(t, ShouldStreamNonStreamingFallback(errors.New("unexpected end of JSON input")))
	require.True(t, ShouldStreamNonStreamingFallback(&ProviderError{Title: "stream stalled"}))
	require.True(t, ShouldStreamNonStreamingFallback(&ProviderError{StatusCode: http.StatusGatewayTimeout}))
	require.False(t, ShouldStreamNonStreamingFallback(&ProviderError{Message: "invalid api key", StatusCode: 401}))
}
