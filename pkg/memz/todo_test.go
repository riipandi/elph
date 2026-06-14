package memz

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestApplyQueryClearAndSet(t *testing.T) {
	ctx := context.Background()
	todos := []Todo{{Title: "one", Status: StatusDone}}
	ctx = WithStore(ctx, &todos)

	out, err := Apply(ctx, nil, false)
	require.NoError(t, err)
	require.Contains(t, out, "[done] one")

	out, err = Apply(ctx, []any{}, true)
	require.NoError(t, err)
	require.Equal(t, "Todo list cleared.", out)
	require.Empty(t, todos)

	ctx = WithStore(ctx, &todos)
	_, err = Apply(ctx, []any{
		map[string]any{"title": "alpha", "status": "pending"},
		map[string]any{"title": "beta", "status": "in_progress"},
	}, true)
	require.NoError(t, err)
	require.Len(t, todos, 2)
	require.Equal(t, StatusInProgress, todos[1].Status)
}

func TestParseTodosArgRejectsInvalidStatus(t *testing.T) {
	_, err := ParseTodosArg([]any{
		map[string]any{"title": "x", "status": "blocked"},
	})
	require.Error(t, err)
}
