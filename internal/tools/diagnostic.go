package tools

// Diagnostic tool names (coding-agent only, not published in pkg/tool).
const (
	ListTools    = "diagnostic_list_tools"
	SystemPrompt = "diagnostic_system_prompt"
	OpenLog      = "diagnostic_open_log"
)

var diagnostic = []Definition{
	{
		Name:            ListTools,
		Category:        CategoryDiagnostic,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "List all tools currently available to the agent",
	},
	{
		Name:            SystemPrompt,
		Category:        CategoryDiagnostic,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Show the assembled system prompt for this session",
	},
	{
		Name:            OpenLog,
		Category:        CategoryDiagnostic,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Open or display the current session log",
	},
}

var byName = func() map[string]Definition {
	m := make(map[string]Definition, len(diagnostic))
	for _, def := range diagnostic {
		m[def.Name] = def
	}
	return m
}()