package agent

import (
	"context"
	"errors"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/ai/utils"
	"github.com/stretchr/testify/require"
)

func TestProviderCancelError(t *testing.T) {
	require.True(t, ProviderCancelError(context.Canceled))
	require.True(t, ProviderCancelError(errors.Join(context.Canceled, errors.New("read stream"))))
	require.True(t, ProviderCancelError(&provider.ProviderError{
		Title:   "stream cancelled",
		Message: "context canceled",
		Cause:   context.Canceled,
	}))
	require.False(t, ProviderCancelError(errors.New("unexpected end of JSON input")))
	require.False(t, ProviderCancelError(utils.ErrStreamStall))
	require.False(t, ProviderCancelError(&provider.ProviderError{
		Title:   "stream stalled",
		Message: "No data received from the provider.",
		Cause:   utils.ErrStreamStall,
	}))
}
