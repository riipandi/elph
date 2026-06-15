package protocol

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestParseUpstreamErrorBodyOpenAI(t *testing.T) {
	t.Parallel()

	body := []byte(`{"error":{"message":"invalid model","type":"invalid_request_error","code":"invalid_request_error"}}`)
	fields, ok := ParseUpstreamErrorBody(body)
	require.True(t, ok)
	require.Equal(t, "invalid model", fields.Message)
	require.Equal(t, "invalid_request_error", fields.Type)
	require.Equal(t, "invalid_request_error", fields.Code)
	require.Equal(t, "invalid_request_error: invalid model", FormatProviderErrorMessage(fields))
}

func TestParseUpstreamErrorBodyStripsGatewayPrefix(t *testing.T) {
	t.Parallel()

	body := []byte(`{"error":{"message":"Error from provider (DeepSeek): unknown variant developer","type":"invalid_request_error"}}`)
	fields, ok := ParseUpstreamErrorBody(body)
	require.True(t, ok)
	require.Equal(t, "invalid_request_error: unknown variant developer", FormatProviderErrorMessage(fields))
}

func TestNewUpstreamHTTPErrorAddsDeveloperRoleHint(t *testing.T) {
	t.Parallel()

	body := []byte(`{"error":{"message":"Error from provider (DeepSeek): messages[0].role: unknown variant developer","type":"invalid_request_error"}}`)
	err := NewUpstreamHTTPError(400, body)
	var pe *ProviderError
	require.True(t, errors.As(err, &pe))
	require.Equal(t, "bad request", pe.Title)
	require.Contains(t, pe.Message, "unknown variant")
	require.NotContains(t, pe.Message, "Error from provider")
	require.Contains(t, pe.Hint, "developer role")
}

func TestProviderErrorFromStreamData(t *testing.T) {
	t.Parallel()

	data := []byte(`{"error":{"message":"model overloaded","type":"server_error"}}`)
	pe, ok := ProviderErrorFromStreamData(data)
	require.True(t, ok)
	require.Contains(t, pe.Message, "model overloaded")
	require.Equal(t, "server_error", pe.ErrorType)
	require.Contains(t, pe.Hint, "overloaded")
}

func TestEnrichProviderErrorAppliesNumericErrorCode(t *testing.T) {
	t.Parallel()

	pe := &ProviderError{
		Message:   "Upstream idle timeout exceeded",
		ErrorCode: "504",
	}
	EnrichProviderError(pe)
	require.Equal(t, 504, pe.StatusCode)
	require.Equal(t, "gateway timeout", pe.Title)
	require.Contains(t, pe.Hint, "gateway timed out")
}

func TestShouldStreamNonStreamingFallbackIdleTimeout(t *testing.T) {
	t.Parallel()

	err := &ProviderError{Message: "Upstream idle timeout exceeded", ErrorCode: "504"}
	EnrichProviderError(err)
	require.True(t, ShouldStreamNonStreamingFallback(err))
}

func TestProviderErrorSummaryIncludesHint(t *testing.T) {
	t.Parallel()

	err := &ProviderError{
		Title:   "bad request",
		Message: "unknown variant developer",
		Hint:    "Disable thinking or choose another model.",
	}
	summary := ProviderErrorSummary(err)
	require.Contains(t, summary, "Provider error (bad request):")
	require.Contains(t, summary, "unknown variant developer")
	require.Contains(t, summary, "Disable thinking")
}
