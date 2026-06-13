package constants

// DefaultKeyBindings describes the key bindings available in the TUI.
var DefaultKeyBindings = map[string]string{
	"Ctrl+C":      "Cancel / Quit",
	"Ctrl+D":      "Exit application",
	"Enter":       "Send message",
	"Ctrl+J":      "Insert newline in input",
	"Shift+Enter": "Insert newline in input",
	"Tab":         "Switch agent mode",
	"Shift+Tab":   "Switch agent mode (reverse)",
	":q / :q!":    "Quit (vim-style)",
}
