package renderer

import (
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/memz"
	"github.com/stretchr/testify/require"
)

func TestTodoPanelShowsTasksWithStatusMarkers(t *testing.T) {
	m := testInputModel(t)
	m.session.Todos = []memz.Todo{
		{Title: "read auth.go", Status: memz.StatusDone},
		{Title: "patch handler", Status: memz.StatusInProgress},
		{Title: "run tests", Status: memz.StatusPending},
	}

	rendered := stripANSI(m.todoPanelView())
	require.Contains(t, rendered, "Tasks")
	require.Contains(t, rendered, "read auth.go")
	require.Contains(t, rendered, "patch handler")
	require.Contains(t, rendered, "run tests")
}

func TestTodoPanelHiddenWhenEmpty(t *testing.T) {
	m := testInputModel(t)
	require.Empty(t, m.todoPanelView())
}

func TestTodoListToolSkipsDetailBox(t *testing.T) {
	m := testInputModel(t)
	call := provider.ToolCall{ID: "call_todo", Name: "TodoList"}
	m = m.beginNativeToolCall(call)
	require.True(t, m.agent.TodoListUpdating)
	require.Empty(t, m.messages)

	m.session.Todos = []memz.Todo{{Title: "a", Status: memz.StatusPending}}
	m = m.finishNativeToolCall(call, agent.ToolRunResult{Output: "updated"})
	require.False(t, m.agent.TodoListUpdating)
	require.Empty(t, m.messages)
}
