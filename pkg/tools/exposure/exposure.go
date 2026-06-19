package exposure

import (
	"strings"

	"github.com/riipandi/elph/pkg/tools/catalog"
)

// ResolveName maps a model-supplied tool name to the canonical built-in name.
func ResolveName(raw string) (canonical string, known bool) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return "Tool", false
	}
	lower := strings.ToLower(trimmed)
	for _, def := range catalog.All() {
		if strings.ToLower(def.Name) == lower {
			return def.Name, true
		}
	}
	return titleCaseToolName(trimmed), false
}

// IsExecutable reports whether the agent runtime can run a built-in tool by name.
// Returns false for unknown tools. See docs/tools.md for the exposure matrix.
func IsExecutable(name string) bool {
	def, ok := catalog.Get(name)
	if !ok {
		return false
	}
	switch def.Name {
	case catalog.Read, catalog.Write, catalog.Edit, catalog.Grep, catalog.Glob,
		catalog.ReadMediaFile, catalog.Bash, catalog.FetchURL, catalog.WebSearch,
		catalog.CodeSearch, catalog.AskUser, catalog.Skill, catalog.TodoList,
		catalog.CreateGoal, catalog.GetGoal, catalog.UpdateGoal, catalog.SetGoalBudget:
		return true
	default:
		return false
	}
}

func titleCaseToolName(name string) string {
	if len(name) == 1 {
		return strings.ToUpper(name)
	}
	return strings.ToUpper(name[:1]) + name[1:]
}
