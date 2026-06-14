package anthropic

import (
	"errors"
	"io"
	"testing"

	provider "github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/stretchr/testify/require"
)

func TestToProviderErrUnexpectedEOF(t *testing.T) {
	err := toProviderErr(io.ErrUnexpectedEOF)
	var pe *provider.ProviderError
	require.True(t, errors.As(err, &pe))
	require.True(t, pe.IsRetriable())
}
