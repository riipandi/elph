package constants

// KeyBinding defines a single keybinding.
type KeyBinding struct {
	Key    string // Keystroke string (e.g. "ctrl+c")
	Action KeyAction
	Label  string
}

// KeyAction represents the action triggered by a keybinding.
type KeyAction string

const (
	ActionQuit              KeyAction = "quit"
	ActionExit              KeyAction = "exit"
	ActionSwitchMode        KeyAction = "switch_mode"
	ActionCycleThink        KeyAction = "cycle_thinking"
	ActionSubmit            KeyAction = "submit"
	ActionNewline           KeyAction = "newline"
	ActionClearInput        KeyAction = "clear_input"
	ActionCopy              KeyAction = "copy"
	ActionExport            KeyAction = "export"
	ActionOpenModelSelector KeyAction = "open_model_selector"
	ActionToggleDetail      KeyAction = "toggle_detail"
	ActionCycleTheme        KeyAction = "cycle_theme"
)

// DefaultKeyBindings defines all keybindings for the TUI.
var DefaultKeyBindings = []KeyBinding{
	{Key: "ctrl+c", Action: ActionQuit, Label: "Cancel / Quit"},
	{Key: "ctrl+x", Action: ActionQuit, Label: "Cancel / Quit"},
	{Key: "ctrl+d", Action: ActionExit, Label: "Exit application"},
	{Key: "ctrl+a", Action: ActionSwitchMode, Label: "Switch agent mode"},
	{Key: "shift+tab", Action: ActionCycleThink, Label: "Cycle thinking level"},
	{Key: "enter", Action: ActionSubmit, Label: "Send message"},
	{Key: "ctrl+j", Action: ActionNewline, Label: "Insert newline in input"},
	{Key: "ctrl+y", Action: ActionCopy, Label: "Copy last message"},
	{Key: "ctrl+l", Action: ActionOpenModelSelector, Label: "Open model selector"},
	{Key: "ctrl+o", Action: ActionToggleDetail, Label: "Expand/collapse detail block"},
	{Key: "ctrl+shift+t", Action: ActionCycleTheme, Label: "Cycle theme (auto/dark/light)"},
}

// KeyBindingsByAction returns a map of action to keybinding for quick lookup.
func KeyBindingsByAction() map[KeyAction]KeyBinding {
	result := make(map[KeyAction]KeyBinding, len(DefaultKeyBindings))
	for _, kb := range DefaultKeyBindings {
		if _, exists := result[kb.Action]; !exists {
			result[kb.Action] = kb
		}
	}
	return result
}

// KeyBindingLabels returns a list of "key: description" strings for display.
func KeyBindingLabels() []string {
	labels := make([]string, 0, len(DefaultKeyBindings))
	for _, kb := range DefaultKeyBindings {
		labels = append(labels, kb.Label)
	}
	return labels
}
