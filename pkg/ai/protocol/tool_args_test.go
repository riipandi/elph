package protocol

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNormalizeToolArguments(t *testing.T) {
	t.Parallel()

	require.Equal(t, json.RawMessage("{}"), NormalizeToolArguments(nil))
	require.Equal(t, json.RawMessage("{}"), NormalizeToolArguments(json.RawMessage("")))
	require.Equal(t, json.RawMessage("{}"), NormalizeToolArguments(json.RawMessage("  ")))
	require.Equal(t, json.RawMessage("{}"), NormalizeToolArguments(json.RawMessage("null")))
	require.Equal(t, json.RawMessage("{}"), NormalizeToolArguments(json.RawMessage(`{}{"question":"hi"}`)))

	raw := json.RawMessage(`{"question":"Pick one"}`)
	require.Equal(t, raw, NormalizeToolArguments(raw))
}
