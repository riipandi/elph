package catalog

// Category groups built-in tools by capability area.
type Category string

const (
	CategoryFile            Category = "file"
	CategoryShell           Category = "shell"
	CategoryWeb             Category = "web"
	CategoryPlanMode        Category = "plan_mode"
	CategoryStateManagement Category = "state_management"
	CategoryCollaboration   Category = "collaboration"
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
	RequiresConfirmation bool
}
