// Package tools defines coding-agent built-in tools that are not published in pkg/tool.
// Diagnostic helpers live here and are only available inside the elph application.
package tools

// Category groups internal-only tools.
type Category string

const (
	CategoryDiagnostic Category = "diagnostic"
)

// Approval is the default policy before a tool runs.
type Approval string

const (
	ApprovalAutoAllow        Approval = "auto-allow"
	ApprovalRequiresApproval Approval = "requires-approval"
)

// Definition describes an internal built-in tool.
type Definition struct {
	Name            string
	Category        Category
	DefaultApproval Approval
	Description     string
}

// Get returns an internal tool definition by name.
func Get(name string) (Definition, bool) {
	def, ok := byName[name]
	return def, ok
}

// Diagnostic returns diagnostic tools in catalog order.
func Diagnostic() []Definition {
	return append([]Definition(nil), diagnostic...)
}

// All returns every internal built-in tool in catalog order.
func All() []Definition {
	return Diagnostic()
}