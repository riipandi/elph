package openai

import (
	"errors"
	"io"
	"testing"

	openaisdk "github.com/openai/openai-go/v3"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/stretchr/testify/require"
)

func TestToProviderErrUnexpectedEOF(t *testing.T) {
	err := toProviderErr(io.ErrUnexpectedEOF)
	var pe *provider.ProviderError
	require.True(t, errors.As(err, &pe))
	require.True(t, pe.IsRetriable())
}

func TestToProviderErrAPI(t *testing.T) {
	apiErr := &openaisdk.Error{StatusCode: 429, Message: "slow down"}
	err := toProviderErr(apiErr)
	var pe *provider.ProviderError
	require.True(t, errors.As(err, &pe))
	require.Equal(t, 429, pe.StatusCode)
	require.Equal(t, "slow down", pe.Message)
	require.Contains(t, pe.Hint, "retry")
	require.True(t, pe.IsRetriable())
}

func TestToProviderErrParsesOpenAIErrorEnvelope(t *testing.T) {
	apiErr := &openaisdk.Error{
		StatusCode: 400,
		Type:       "invalid_request_error",
		Message:    "Error from provider (DeepSeek): unknown variant `developer`",
	}
	err := toProviderErr(apiErr)
	var pe *provider.ProviderError
	require.True(t, errors.As(err, &pe))
	require.Equal(t, "invalid_request_error: unknown variant `developer`", pe.Message)
	require.Contains(t, pe.Hint, "developer role")
}
