package tools

// Diagnostic tool names (coding-agent only, not published in pkg/tool).
const (
	DiagnosticListTools    = "DiagnosticListTools"
	DiagnosticSystemPrompt = "DiagnosticSystemPrompt"
	DiagnosticOpenLog      = "DiagnosticOpenLog"
)

var diagnostic = []Definition{
	{
		Name:            DiagnosticListTools,
		Category:        CategoryDiagnostic,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "List all tools currently available to the agent",
	},
	{
		Name:            DiagnosticSystemPrompt,
		Category:        CategoryDiagnostic,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Show the assembled system prompt for this session",
	},
	{
		Name:            DiagnosticOpenLog,
		Category:        CategoryDiagnostic,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Open a session log (requests or system)",
	},
}

var byName = func() map[string]Definition {
	m := make(map[string]Definition, len(diagnostic))
	for _, def := range diagnostic {
		m[def.Name] = def
	}
	return m
}()
