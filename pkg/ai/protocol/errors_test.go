package protocol

import (
	"encoding/json"
	"errors"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestIsStreamJSONError(t *testing.T) {
	t.Parallel()

	require.False(t, IsStreamJSONError(nil))
	require.True(t, IsStreamJSONError(&json.SyntaxError{}))
	require.True(t, IsStreamJSONError(errors.New("unexpected end of JSON input")))
	require.False(t, IsStreamJSONError(errors.New("connection reset")))
}
