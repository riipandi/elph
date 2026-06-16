package log

import (
	"bufio"
	"encoding/json"
	"os"
	"strings"
	"time"

	"github.com/riipandi/elph/internal/projectdir"
	"github.com/riipandi/elph/pkg/tools/todolist"
)

type todosSnapshotRecord struct {
	Time  string          `json:"time,omitempty"`
	Todos []todolist.Todo `json:"todos"`
}

// LoadTodos returns the latest todo snapshot from
// <workDir>/.agents/elph/metadata/<sessionID>/todos.jsonl.
func LoadTodos(workDir, sessionID string) ([]todolist.Todo, error) {
	if workDir == "" || sessionID == "" {
		return nil, nil
	}
	path := projectdir.SessionTodosPath(workDir, sessionID)
	f, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	defer f.Close()

	var latest []todolist.Todo
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		var rec todosSnapshotRecord
		if err := json.Unmarshal([]byte(line), &rec); err != nil {
			// Back-compat with older records that included session metadata.
			var legacy struct {
				Todos []todolist.Todo `json:"todos"`
			}
			if err := json.Unmarshal([]byte(line), &legacy); err != nil {
				continue
			}
			latest = append([]todolist.Todo(nil), legacy.Todos...)
			continue
		}
		latest = append([]todolist.Todo(nil), rec.Todos...)
	}
	if err := scanner.Err(); err != nil {
		return nil, err
	}
	return latest, nil
}

// SaveTodosSnapshot persists the current todo list, replacing any prior snapshot.
// Empty lists remove the session file so stale state does not linger on disk.
func SaveTodosSnapshot(workDir, sessionID string, todos []todolist.Todo) error {
	if workDir == "" || sessionID == "" {
		return nil
	}
	path := projectdir.SessionTodosPath(workDir, sessionID)
	if len(todos) == 0 {
		err := os.Remove(path)
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if err := projectdir.EnsureSessionMetadataDir(workDir, sessionID); err != nil {
		return err
	}

	rec := todosSnapshotRecord{
		Time:  time.Now().UTC().Format(time.RFC3339),
		Todos: append([]todolist.Todo(nil), todos...),
	}
	data, err := json.Marshal(rec)
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(data, '\n'), 0o600)
}

// TodosEqual reports whether two todo lists have the same titles and statuses.
func TodosEqual(a, b []todolist.Todo) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i].Title != b[i].Title || a[i].Status != b[i].Status {
			return false
		}
	}
	return true
}
