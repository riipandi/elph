package command

var builtin = []SlashCommand{
	{
		Name:        "help",
		Description: "Show available slash commands",
	},
	{
		Name:        "changelog",
		Description: "Show version history",
		Handler:     notImplemented("changelog"),
	},
	{
		Name:        "model",
		Description: "Switch the active AI model",
		Handler:     modelHandler,
	},
	{
		Name:        "settings",
		Aliases:     []string{"config"},
		Description: "Open the configuration panel",
		Handler:     notImplemented("settings"),
	},
	{
		Name:        "diff",
		Description: "View uncommitted workspace changes",
		Handler:     notImplemented("diff"),
	},
	{
		Name:        DiagnosticListTools,
		Description: "List all tools available to the agent",
		Handler:     diagnosticListTools,
	},
	{
		Name:        DiagnosticOpenLog,
		Description: "Open a session log (requests or system)",
		Args:        openLogArgs,
		Handler:     diagnosticOpenLog,
	},
	{
		Name:        DiagnosticDebug,
		Description: "Show diagnostic information",
		Handler:     diagnosticDebug,
	},
	{
		Name:        "exit",
		Aliases:     []string{"quit", "q"},
		Description: "Exit the application",
		Quits:       true,
		Handler:     func(*Context, string) string { return "" },
	},
	{
		Name:        "compact",
		Aliases:     []string{"c"},
		Description: "Compress conversation history to save context window space",
		Handler:     compactHandler,
	},
	{
		Name:        "context",
		Description: "View context usage and token breakdown",
		Handler:     contextHandler,
	},
	{
		Name:         "goal",
		Aliases:      []string{"goals"},
		Description:  "Manage session goals: status, pause, resume, cancel, replace, next",
		ArgumentHint: "<subcommand> [args]",
		Handler:      goalHandler,
	},
}
