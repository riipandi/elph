package constants

// DefaultKeyBindings describes the key bindings available in the TUI.
var DefaultKeyBindings = map[string]string{
	"Ctrl+C":      "Cancel / Quit",
	"Ctrl+X":      "Cancel / Quit",
	"Ctrl+D":      "Exit application",
	"Alt+M":       "Switch agent mode",
	"Enter":       "Send message",
	"Ctrl+J":      "Insert newline in input",
	"Shift+Enter": "Insert newline in input",
	"Shift+Tab":   "Cycle thinking level",
	":q / :q!":    "Quit (vim-style)",
}
