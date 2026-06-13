package runtime

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNewSessionHasID(t *testing.T) {
	s := NewSession()
	require.NotEmpty(t, s.ID.String())
}

func TestSessionRunTurnReturnsCommand(t *testing.T) {
	s := NewSession()
	require.NotNil(t, s.RunTurn("hello"))
}