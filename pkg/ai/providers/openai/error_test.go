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
	require.True(t, pe.IsRetriable())
}
