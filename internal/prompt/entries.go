package prompt

import (
	"github.com/riipandi/elph/internal/tools"
	"github.com/riipandi/elph/pkg/tool"
)

type catalogEntry struct {
	name                 string
	section              string
	defaultApproval      string
	description          string
	requiresConfirmation bool
}

var pkgSectionByCategory = map[tool.Category]string{
	tool.CategoryFile:          "File Tools",
	tool.CategoryShell:         "Shell Tools",
	tool.CategoryWeb:           "Web Tools",
	tool.CategoryPlanMode:      "Plan Mode",
	tool.CategoryCollaboration: "Collaboration Tools",
}

var pkgCategoryOrder = []tool.Category{
	tool.CategoryFile,
	tool.CategoryShell,
	tool.CategoryWeb,
	tool.CategoryPlanMode,
	tool.CategoryCollaboration,
}

func catalogEntries(explicit []tool.Definition) []catalogEntry {
	pkgTools := explicit
	if pkgTools == nil {
		pkgTools = tool.All()
	}

	entries := make([]catalogEntry, 0, len(pkgTools)+len(tools.Diagnostic()))

	for _, category := range pkgCategoryOrder {
		section := pkgSectionByCategory[category]
		for _, def := range pkgTools {
			if def.Category != category {
				continue
			}
			entries = append(entries, catalogEntry{
				name:                 def.Name,
				section:              section,
				defaultApproval:      string(def.DefaultApproval),
				description:          def.Description,
				requiresConfirmation: def.RequiresConfirmation,
			})
		}
	}

	for _, def := range tools.Diagnostic() {
		entries = append(entries, catalogEntry{
			name:            def.Name,
			section:         "Diagnostic Tools",
			defaultApproval: string(def.DefaultApproval),
			description:     def.Description,
		})
	}

	return entries
}