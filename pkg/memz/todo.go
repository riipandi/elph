package memz

import (
	"context"
	"fmt"
	"strings"
)

// Status is a todo item lifecycle state.
type Status string

const (
	StatusPending    Status = "pending"
	StatusInProgress Status = "in_progress"
	StatusDone       Status = "done"
)

// Todo is a single tracked subtask.
type Todo struct {
	Title  string
	Status Status
}

type storeKey struct{}

// WithStore attaches session-scoped todo storage to ctx.
func WithStore(ctx context.Context, todos *[]Todo) context.Context {
	if todos == nil {
		return ctx
	}
	return context.WithValue(ctx, storeKey{}, todos)
}

// StoreFrom returns the todo slice pointer bound to ctx, if any.
func StoreFrom(ctx context.Context) *[]Todo {
	if ctx == nil {
		return nil
	}
	store, _ := ctx.Value(storeKey{}).(*[]Todo)
	return store
}

// Get returns the current todo list from ctx.
func Get(ctx context.Context) []Todo {
	store := StoreFrom(ctx)
	if store == nil {
		return nil
	}
	if len(*store) == 0 {
		return nil
	}
	out := make([]Todo, len(*store))
	copy(out, *store)
	return out
}

// Apply updates session todos per TodoList tool semantics.
// When todosArg is absent (present=false), the list is queried.
// When todosArg is an empty array, the list is cleared.
func Apply(ctx context.Context, todosArg any, present bool) (string, error) {
	store := StoreFrom(ctx)
	if store == nil {
		return "", fmt.Errorf("todo store unavailable")
	}
	if !present {
		return FormatList(*store), nil
	}
	if todosArg == nil {
		return FormatList(*store), nil
	}
	items, err := ParseTodosArg(todosArg)
	if err != nil {
		return "", err
	}
	if len(items) == 0 {
		*store = nil
		return "Todo list cleared.", nil
	}
	*store = items
	return FormatList(*store), nil
}

// ParseTodosArg decodes the todos tool argument.
func ParseTodosArg(raw any) ([]Todo, error) {
	arr, ok := raw.([]any)
	if !ok {
		return nil, fmt.Errorf("todos must be an array")
	}
	if len(arr) == 0 {
		return nil, nil
	}
	out := make([]Todo, 0, len(arr))
	for i, item := range arr {
		m, ok := item.(map[string]any)
		if !ok {
			return nil, fmt.Errorf("todos[%d] must be an object", i)
		}
		title, ok := stringField(m, "title")
		if !ok {
			return nil, fmt.Errorf("todos[%d] missing title", i)
		}
		statusRaw, ok := stringField(m, "status")
		if !ok {
			return nil, fmt.Errorf("todos[%d] missing status", i)
		}
		status, err := ParseStatus(statusRaw)
		if err != nil {
			return nil, fmt.Errorf("todos[%d]: %w", i, err)
		}
		out = append(out, Todo{Title: title, Status: status})
	}
	return out, nil
}

// ParseStatus validates a todo status string.
func ParseStatus(raw string) (Status, error) {
	switch Status(strings.TrimSpace(raw)) {
	case StatusPending, StatusInProgress, StatusDone:
		return Status(strings.TrimSpace(raw)), nil
	default:
		return "", fmt.Errorf("invalid status %q (want pending, in_progress, or done)", raw)
	}
}

// FormatList renders todos for tool output.
func FormatList(todos []Todo) string {
	if len(todos) == 0 {
		return "No todos."
	}
	var b strings.Builder
	for i, item := range todos {
		if i > 0 {
			b.WriteByte('\n')
		}
		fmt.Fprintf(&b, "[%s] %s", item.Status, item.Title)
	}
	return b.String()
}

func stringField(m map[string]any, key string) (string, bool) {
	raw, ok := m[key]
	if !ok || raw == nil {
		return "", false
	}
	switch v := raw.(type) {
	case string:
		s := strings.TrimSpace(v)
		return s, s != ""
	default:
		s := strings.TrimSpace(fmt.Sprint(v))
		return s, s != ""
	}
}
