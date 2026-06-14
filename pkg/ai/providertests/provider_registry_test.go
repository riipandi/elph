package providertests

import (
	"testing"

	elphant "github.com/riipandi/elph/pkg/ai/providers/anthropic"
	elphopenai "github.com/riipandi/elph/pkg/ai/providers/openai"
	elphcompat "github.com/riipandi/elph/pkg/ai/providers/openaicompat"
	elphor "github.com/riipandi/elph/pkg/ai/providers/openrouter"
	"github.com/stretchr/testify/require"
)

func TestProviderNames(t *testing.T) {
	require.Equal(t, "openai", elphopenai.Name)
	require.Equal(t, "openai-compat", elphcompat.Name)
	require.Equal(t, "openrouter", elphor.Name)
	require.Equal(t, "anthropic", elphant.Name)
}
