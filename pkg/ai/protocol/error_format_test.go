package protocol

import (
	"errors"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestProviderErrorSummary(t *testing.T) {
	t.Parallel()
	require.Equal(t, "", ProviderErrorSummary(nil))
	require.Equal(t, "Provider error: boom", ProviderErrorSummary(errors.New("boom")))
}

func TestFormatProviderErrorDetailPlain(t *testing.T) {
	t.Parallel()
	got := FormatProviderErrorDetail(errors.New("unexpected end of JSON input"))
	require.Contains(t, got, "Provider request failed")
	require.Contains(t, got, "unexpected end of JSON input")
}

func TestFormatProviderErrorDetailStructured(t *testing.T) {
	t.Parallel()
	err := &ProviderError{
		Title:      "bad request",
		Message:    "invalid model",
		StatusCode: 400,
		URL:        "https://api.example.com/v1/chat/completions",
		RequestBody:  []byte(`{"model":"test"}`),
		ResponseBody: []byte(`{"error":"nope"}`),
		ResponseHeaders: map[string]string{
			"X-Request-Id": "req-1",
		},
	}
	got := FormatProviderErrorDetail(err)
	require.Contains(t, got, "invalid model")
	require.Contains(t, got, "HTTP 400")
	require.Contains(t, got, "https://api.example.com/v1/chat/completions")
	require.Contains(t, got, "--- Request ---")
	require.Contains(t, got, `{"model":"test"}`)
	require.Contains(t, got, "--- Response ---")
	require.Contains(t, got, `{"error":"nope"}`)
	require.Contains(t, got, "X-Request-Id: req-1")
}

func TestFormatProviderErrorDetailTruncatesLargeBodies(t *testing.T) {
	t.Parallel()
	err := &ProviderError{
		Message:      "upstream error",
		ResponseBody: []byte(strings.Repeat("x", maxProviderErrorBodyBytes+100)),
	}
	got := FormatProviderErrorDetail(err)
	require.Contains(t, got, "[truncated — showing first")
}