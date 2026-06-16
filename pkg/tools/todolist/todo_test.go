package todolist

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

func TestHasActiveAndAllDone(t *testing.T) {
	require.False(t, HasActive(nil))
	require.False(t, AllDone(nil))
	require.True(t, AllDone([]Todo{{Title: "a", Status: StatusDone}}))

	active := []Todo{
		{Title: "a", Status: StatusDone},
		{Title: "b", Status: StatusPending},
	}
	require.True(t, HasActive(active))
	require.False(t, AllDone(active))

	done := []Todo{
		{Title: "a", Status: StatusDone},
		{Title: "b", Status: StatusDone},
	}
	require.False(t, HasActive(done))
	require.True(t, AllDone(done))
}

func TestParseTodosArgRejectsInvalidStatus(t *testing.T) {
	_, err := ParseTodosArg([]any{
		map[string]any{"title": "x", "status": "blocked"},
	})
	require.Error(t, err)
}

func TestApplyNoStoreReturnsError(t *testing.T) {
	ctx := context.Background()
	_, err := Apply(ctx, nil, false)
	require.Error(t, err)
	require.Contains(t, err.Error(), "todo store unavailable")
}

func TestApplyPresentTrueNilArgQueries(t *testing.T) {
	todos := []Todo{{Title: "task", Status: StatusPending}}
	ctx := WithStore(context.Background(), &todos)
	out, err := Apply(ctx, nil, true)
	require.NoError(t, err)
	require.Contains(t, out, "[pending] task")
}

func TestGetNilContext(t *testing.T) {
	require.Nil(t, Get(context.TODO()))
}

func TestGetEmptyStore(t *testing.T) {
	var todos []Todo
	ctx := WithStore(context.Background(), &todos)
	require.Nil(t, Get(ctx))
}

func TestFormatListEmpty(t *testing.T) {
	require.Equal(t, "No todos.", FormatList(nil))
}

func TestFormatListMultiple(t *testing.T) {
	todos := []Todo{
		{Title: "a", Status: StatusPending},
		{Title: "b", Status: StatusDone},
	}
	out := FormatList(todos)
	require.Contains(t, out, "[pending] a")
	require.Contains(t, out, "[done] b")
}

func TestParseStatusValid(t *testing.T) {
	for _, tc := range []struct {
		raw  string
		want Status
	}{
		{"pending", StatusPending},
		{"in_progress", StatusInProgress},
		{"done", StatusDone},
	} {
		s, err := ParseStatus(tc.raw)
		require.NoError(t, err)
		require.Equal(t, tc.want, s)
	}
}

func TestParseStatusInvalid(t *testing.T) {
	_, err := ParseStatus("unknown")
	require.Error(t, err)
}

func TestParseTodosArgNotArray(t *testing.T) {
	_, err := ParseTodosArg("not an array")
	require.Error(t, err)
}

func TestParseTodosArgItemMissingTitle(t *testing.T) {
	_, err := ParseTodosArg([]any{
		map[string]any{"status": "pending"},
	})
	require.Error(t, err)
}

func TestParseTodosArgItemMissingStatus(t *testing.T) {
	_, err := ParseTodosArg([]any{
		map[string]any{"title": "x"},
	})
	require.Error(t, err)
}

func TestWithStoreNilReturnsContext(t *testing.T) {
	ctx := context.Background()
	result := WithStore(ctx, nil)
	require.Equal(t, ctx, result)
}

func TestStoreFromNilContext(t *testing.T) {
	require.Nil(t, StoreFrom(context.TODO()))
}
