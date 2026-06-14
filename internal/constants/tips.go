package constants

// Tips is a collection of tips shown in the TUI banner on random.
var Tips = []string{
	"Use /help to see all available slash commands.",
	"Press Ctrl+C once to cancel, twice to exit.",
	"Type :q and press Enter to quit (vim-style exit).",
	"Press Ctrl+D to exit the application.",
	"Press Ctrl+L or type /model to switch the active AI model.",
	"Press Ctrl+A to cycle agent modes (build, plan, ask, brave).",
	"Press Shift+Tab to cycle thinking level.",
	"Press Ctrl+Shift+T to cycle theme (auto, dark, light).",
	"Press Ctrl+Y to copy the last message.",
	"Use /diagnostic:system-prompt to inspect the assembled system prompt.",
	"Use /diagnostic:list-tools to see tools available to the agent.",
	"Session logs are written to <workDir>/.agents/elph/logs/<session>/ — see /diagnostic:open-log.",
	"Run elph provider connect to create starter provider configs under ~/.elph/providers.",
}
