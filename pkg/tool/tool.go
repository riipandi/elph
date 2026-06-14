// Package tool defines built-in coding-agent tools as a publishable library.
// Tool execution and approval UI live in internal/ (coding-agent); this package
// holds names, categories, and default approval policy from docs/tools.md.
package tool

// Category groups built-in tools by capability area.
type Category string

const (
	CategoryFile          Category = "file"
	CategoryShell         Category = "shell"
	CategoryWeb           Category = "web"
	CategoryPlanMode      Category = "plan_mode"
	CategoryCollaboration Category = "collaboration"
)

// Approval is the default policy before a tool runs.
type Approval string

const (
	ApprovalAutoAllow        Approval = "auto-allow"
	ApprovalRequiresApproval Approval = "requires-approval"
	ApprovalAlwaysApprove    Approval = "always-approve"
)

// Definition describes a built-in tool's metadata.
type Definition struct {
	Name                 string
	Category             Category
	DefaultApproval      Approval
	Description          string
	RequiresConfirmation bool // extra user confirmation after the tool completes
}

// Get returns a built-in tool definition by name.
func Get(name string) (Definition, bool) {
	def, ok := builtinByName[name]
	return def, ok
}

// All returns every built-in tool in catalog order.
func All() []Definition {
	return append([]Definition(nil), builtin...)
}

// ByCategory returns built-in tools in the given category, in catalog order.
func ByCategory(category Category) []Definition {
	out := make([]Definition, 0)
	for _, def := range builtin {
		if def.Category == category {
			out = append(out, def)
		}
	}
	return out
}

// Names returns built-in tool names in catalog order.
func Names() []string {
	names := make([]string, len(builtin))
	for i, def := range builtin {
		names[i] = def.Name
	}
	return names
}

// RequiresApproval reports whether a tool defaults to requiring user approval.
func RequiresApproval(name string) bool {
	def, ok := builtinByName[name]
	return ok && def.DefaultApproval == ApprovalRequiresApproval
}
