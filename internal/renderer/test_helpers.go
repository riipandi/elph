package renderer

import tea "charm.land/bubbletea/v2"

func viewContent(m Model) string {
	return m.View().Content
}

func keyCtrl(ch rune) tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: ch, Mod: tea.ModCtrl}
}

func keyAlt(ch rune) tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: ch, Mod: tea.ModAlt}
}

func keyEnter() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyEnter}
}

func keyRune(ch rune) tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: ch, Text: string(ch)}
}

func keyShiftTab() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift}
}

func keyShiftDown() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyDown, Mod: tea.ModShift}
}

func keyShiftUp() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyUp, Mod: tea.ModShift}
}

func keyDown() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyDown}
}

func keyUp() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyUp}
}

func keyLeft() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyLeft}
}

func keyRight() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyRight}
}

func keyTab() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyTab}
}

func keyCtrlJ() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: 'j', Mod: tea.ModCtrl}
}

func keyCtrlShiftT() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: 't', Mod: tea.ModCtrl | tea.ModShift}
}

func mouseClick(x, y int, btn tea.MouseButton, mod tea.KeyMod) tea.MouseClickMsg {
	return tea.MouseClickMsg{X: x, Y: y, Button: btn, Mod: mod}
}

func mouseWheel(x, y int, btn tea.MouseButton) tea.MouseWheelMsg {
	return tea.MouseWheelMsg{X: x, Y: y, Button: btn}
}

func mouseRelease(x, y int, btn tea.MouseButton) tea.MouseReleaseMsg {
	return tea.MouseReleaseMsg{X: x, Y: y, Button: btn}
}
