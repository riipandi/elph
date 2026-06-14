package providertests

import (
	"testing"

	elphcompat "github.com/riipandi/elph/pkg/ai/providers/openaicompat"
	"github.com/stretchr/testify/require"
)

func TestOpenAICompatName(t *testing.T) {
	require.Equal(t, "openai-compat", elphcompat.Name)
}
