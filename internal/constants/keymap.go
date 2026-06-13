package constants

import "golang.design/x/hotkey"

// KeyAction represents the action triggered by a keybinding.
type KeyAction string

const (
	ActionQuit       KeyAction = "quit"
	ActionExit       KeyAction = "exit"
	ActionSwitchMode KeyAction = "switch_mode"
	ActionCycleThink KeyAction = "cycle_thinking"
	ActionSubmit     KeyAction = "submit"
	ActionNewline    KeyAction = "newline"
	ActionClearInput KeyAction = "clear_input"
)

// KeyBinding defines a single keybinding with modifiers, key, and action.
type KeyBinding struct {
	Modifiers []hotkey.Modifier
	Key       hotkey.Key
	Action    KeyAction
	Label     string // Human-readable label for display
}

// DefaultKeyBindings defines all keybindings for the TUI.
var DefaultKeyBindings = []KeyBinding{
	// Quit / Exit
	{
		Modifiers: []hotkey.Modifier{hotkey.ModCtrl},
		Key:       hotkey.KeyC,
		Action:    ActionQuit,
		Label:     "Cancel / Quit",
	},
	{
		Modifiers: []hotkey.Modifier{hotkey.ModCtrl},
		Key:       hotkey.KeyX,
		Action:    ActionQuit,
		Label:     "Cancel / Quit",
	},
	{
		Modifiers: []hotkey.Modifier{hotkey.ModCtrl},
		Key:       hotkey.KeyD,
		Action:    ActionExit,
		Label:     "Exit application",
	},
	// Mode switching
	{
		Modifiers: []hotkey.Modifier{hotkey.ModCtrl},
		Key:       hotkey.KeyA,
		Action:    ActionSwitchMode,
		Label:     "Switch agent mode",
	},
	// Thinking level
	{
		Modifiers: []hotkey.Modifier{hotkey.ModShift},
		Key:       hotkey.KeyTab,
		Action:    ActionCycleThink,
		Label:     "Cycle thinking level",
	},
	// Input actions (handled specially in TUI context)
	{
		Modifiers: []hotkey.Modifier{},
		Key:       hotkey.KeyReturn,
		Action:    ActionSubmit,
		Label:     "Send message",
	},
	{
		Modifiers: []hotkey.Modifier{hotkey.ModCtrl},
		Key:       hotkey.KeyJ,
		Action:    ActionNewline,
		Label:     "Insert newline in input",
	},
}

// KeyBindingsByAction returns a map of action to keybinding for quick lookup.
func KeyBindingsByAction() map[KeyAction]KeyBinding {
	result := make(map[KeyAction]KeyBinding, len(DefaultKeyBindings))
	for _, kb := range DefaultKeyBindings {
		// Only keep first binding per action
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
