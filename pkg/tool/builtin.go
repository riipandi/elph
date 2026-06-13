package tool

// Built-in tool names (see docs/tools.md).
const (
	Read          = "Read"
	Write         = "Write"
	Edit          = "Edit"
	Grep          = "Grep"
	Glob          = "Glob"
	ReadMediaFile = "ReadMediaFile"
	Bash          = "Bash"
	FetchURL      = "FetchURL"
	WebSearch     = "WebSearch"
	CodeSearch    = "CodeSearch"
	EnterPlanMode = "EnterPlanMode"
	ExitPlanMode  = "ExitPlanMode"
	AskUser       = "AskUser"
)

var builtin = []Definition{
	// File tools
	{
		Name:            Read,
		Category:        CategoryFile,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Read a text file's contents",
	},
	{
		Name:            Write,
		Category:        CategoryFile,
		DefaultApproval: ApprovalRequiresApproval,
		Description:     "Create or overwrite a file",
	},
	{
		Name:            Edit,
		Category:        CategoryFile,
		DefaultApproval: ApprovalRequiresApproval,
		Description:     "Precise string replacement",
	},
	{
		Name:            Grep,
		Category:        CategoryFile,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "ripgrep powered full-text search",
	},
	{
		Name:            Glob,
		Category:        CategoryFile,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Find files by glob pattern",
	},
	{
		Name:            ReadMediaFile,
		Category:        CategoryFile,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Read an image or video file",
	},

	// Shell tools
	{
		Name:            Bash,
		Category:        CategoryShell,
		DefaultApproval: ApprovalRequiresApproval,
		Description:     "Execute a shell command",
	},

	// Web tools
	{
		Name:            FetchURL,
		Category:        CategoryWeb,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Fetch the content of a specified URL",
	},
	{
		Name:            WebSearch,
		Category:        CategoryWeb,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Web search with multiple engines",
	},
	{
		Name:            CodeSearch,
		Category:        CategoryWeb,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Search code on GitHub",
	},

	// Plan mode
	{
		Name:            EnterPlanMode,
		Category:        CategoryPlanMode,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Enter Plan mode",
	},
	{
		Name:            ExitPlanMode,
		Category:        CategoryPlanMode,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Exit Plan mode and submit the plan",
		RequiresConfirmation: true,
	},

	// Collaboration
	{
		Name:            AskUser,
		Category:        CategoryCollaboration,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Ask the user a question to gather structured input",
	},
}

var builtinByName = func() map[string]Definition {
	m := make(map[string]Definition, len(builtin))
	for _, def := range builtin {
		m[def.Name] = def
	}
	return m
}()