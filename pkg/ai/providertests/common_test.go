package providertests

import (
	"context"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

type builderFunc func(t *testing.T) provider.Provider

func testSimpleComplete(t *testing.T, build builderFunc, want string) {
	t.Helper()
	p := build(t)
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		SystemPrompt: "sys",
		UserPrompt:   "hi",
		Model:        "test-model",
	})
	require.NoError(t, err)
	require.Equal(t, want, got.Content)
}
