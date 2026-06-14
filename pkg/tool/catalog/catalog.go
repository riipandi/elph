package catalog

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
		Description:     "Search code on GitHub (token optional) or GitLab",
	},

	// Plan mode
	{
		Name:            EnterPlanMode,
		Category:        CategoryPlanMode,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Enter Plan mode",
	},
	{
		Name:                 ExitPlanMode,
		Category:             CategoryPlanMode,
		DefaultApproval:      ApprovalAutoAllow,
		Description:          "Exit Plan mode and submit the plan",
		RequiresConfirmation: true,
	},

	// State management
	{
		Name:            TodoList,
		Category:        CategoryStateManagement,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Manage a task to-do list",
	},

	// Collaboration
	{
		Name:            AskUser,
		Category:        CategoryCollaboration,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Ask the user a question to gather structured input",
	},
	{
		Name:            Skill,
		Category:        CategoryCollaboration,
		DefaultApproval: ApprovalAutoAllow,
		Description:     "Invoke a registered inline Skill",
	},
}

var builtinByName = func() map[string]Definition {
	m := make(map[string]Definition, len(builtin))
	for _, def := range builtin {
		m[def.Name] = def
	}
	return m
}()

// All returns every built-in tool in catalog order.
func All() []Definition {
	return append([]Definition(nil), builtin...)
}

// Get returns a built-in tool definition by name.
func Get(name string) (Definition, bool) {
	def, ok := builtinByName[name]
	return def, ok
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
