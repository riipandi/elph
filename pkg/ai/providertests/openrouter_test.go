package providertests

import (
	"testing"

	elphor "github.com/riipandi/elph/pkg/ai/providers/openrouter"
	"github.com/stretchr/testify/require"
)

func TestOpenRouterDefaultURL(t *testing.T) {
	require.Equal(t, "https://openrouter.ai/api/v1", elphor.DefaultURL)
}
