package renderer

import (
	"fmt"
	"os"
	"reflect"
	"regexp"
	"strconv"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/command"
)

const (
	maxInputLines    = 8
	minViewportRows  = 6
	inputChromeSlack = 2

	kittyModShift = 1
	kittyModCtrl  = 4
	xtermModShift = 2
	xtermModCtrl  = 5
)

var (
	kittyEnterModsRe   = regexp.MustCompile(`13;(\d+)u`)
	xtermEnterModsRe   = regexp.MustCompile(`27;(\d+);13`)
	kittyCtrlJRe       = regexp.MustCompile(`^(?:10|106);(\d+)u$`)
	xtermCtrlJRe       = regexp.MustCompile(`^27;(\d+);(?:10|106)~?$`)
	legacyShiftEnterRe = regexp.MustCompile(`13;2~`)
	csiByteListRe      = regexp.MustCompile(`^\[([0-9]+(?: [0-9]+)*)\]$`)
)

type termFeaturesMsg struct{}

// activateTerminalFeaturesSync enables enhanced key reporting before the program
// starts so the first keystroke already uses modifyOtherKeys / Kitty protocol.
func activateTerminalFeaturesSync() {
	// modifyOtherKeys makes Option+Delete report as CSI 27;3;127~ in xterm-compatible
	// terminals (Ghostty, VS Code). Kitty keyboard protocol disabled here because it
	// can prevent those legacy/modifyOtherKeys sequences from being delivered.
	_, _ = fmt.Fprint(os.Stdout, ansi.SetModifyOtherKeys2)
}

// enableTerminalFeatures requests enhanced key reporting so Shift+Enter can be
// distinguished from Enter. Uses push semantics to preserve user terminal cfg.
func enableTerminalFeatures() tea.Cmd {
	return func() tea.Msg {
		activateTerminalFeaturesSync()
		return termFeaturesMsg{}
	}
}

func disableTerminalFeatures() tea.Cmd {
	return func() tea.Msg {
		_, _ = fmt.Fprint(os.Stdout, ansi.ResetModifyOtherKeys)
		return nil
	}
}

// inputContentWidth is the textarea width inside the input border and padding.
func inputContentWidth(outer int) int {
	return max(outer-4, 1)
}

// overlayInputScrollBar replaces the last column of each textarea line with the
// scrollbar track/thumb. Lines are padded to targetWidth so the bar sits flush
// against the right border padding even when textarea lines are shorter.
func overlayInputScrollBar(body, bar string, targetWidth int) string {
	bodyLines := strings.Split(body, "\n")
	barLines := strings.Split(bar, "\n")
	if len(bodyLines) > 0 && bodyLines[len(bodyLines)-1] == "" {
		bodyLines = bodyLines[:len(bodyLines)-1]
	}
	if len(barLines) > 0 && barLines[len(barLines)-1] == "" {
		barLines = barLines[:len(barLines)-1]
	}

	textW := max(targetWidth-1, 0)

	out := make([]string, len(bodyLines))
	for i, line := range bodyLines {
		if i >= len(barLines) || barLines[i] == "" {
			out[i] = line
			continue
		}
		truncated := ansi.Truncate(line, textW, "")
		pad := textW - lipgloss.Width(truncated)
		if pad < 0 {
			pad = 0
		}
		out[i] = truncated + strings.Repeat(" ", pad) + barLines[i]
	}
	return strings.Join(out, "\n")
}

func isInputNewlineKey(msg tea.KeyPressMsg) bool {
	if msg.String() == "ctrl+j" || (msg.Code == 'j' && msg.Mod.Contains(tea.ModCtrl)) {
		return true
	}
	return isShiftEnterKeyMsg(msg) || isLiteralNewlineKeyMsg(msg)
}

func isShiftEnterKeyMsg(msg tea.KeyPressMsg) bool {
	return msg.String() == "shift+enter"
}

// isLiteralNewlineKeyMsg matches Ghostty's `keybind = shift+enter=text:\n` and
// VS Code's configured Shift+Enter that inject a newline rune.
func isLiteralNewlineKeyMsg(msg tea.KeyPressMsg) bool {
	return len(msg.Text) == 1 && msg.Text[0] == '\n'
}

func rawCSIPayload(msg tea.Msg) (string, bool) {
	v := reflect.ValueOf(msg)
	if v.Kind() != reflect.Slice || v.Type().Elem().Kind() != reflect.Uint8 {
		return "", false
	}
	b := v.Bytes()
	if len(b) < 3 || b[0] != '\x1b' || b[1] != '[' {
		return "", false
	}
	return string(b[2:]), true
}

func csiPayloadFromString(text string) string {
	i := strings.Index(text, "CSI")
	if i < 0 {
		return ""
	}
	payload := strings.TrimSuffix(text[i+3:], "?")
	if m := csiByteListRe.FindStringSubmatch(payload); len(m) == 2 {
		return decodeCSIByteList(m[1])
	}
	return payload
}

func decodeCSIByteList(list string) string {
	parts := strings.Fields(list)
	buf := make([]byte, 0, len(parts))
	for _, p := range parts {
		n, err := strconv.Atoi(p)
		if err != nil || n < 0 || n > 255 {
			return ""
		}
		buf = append(buf, byte(n))
	}
	return string(buf)
}

func csiPayload(msg tea.Msg) string {
	if payload, ok := rawCSIPayload(msg); ok {
		return payload
	}
	if s, ok := msg.(interface{ String() string }); ok {
		return csiPayloadFromString(s.String())
	}
	return ""
}

func isShiftEnterPayload(payload string) bool {
	if payload == "" {
		return false
	}
	// Kitty: ESC [ 13 ; <mods> u — bit 1 is shift.
	if m := kittyEnterModsRe.FindStringSubmatch(payload); len(m) == 2 {
		mods, err := strconv.Atoi(m[1])
		if err == nil && mods&kittyModShift != 0 {
			return true
		}
		// Ghostty commonly emits 13;2u for Shift+Enter keybind CSI mode.
		if mods == 2 {
			return true
		}
	}
	// xterm modifyOtherKeys: ESC [ 27 ; <mod> ; 13 ~
	if m := xtermEnterModsRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && (mod == xtermModShift || mod == 4) {
			return true
		}
	}
	// Legacy xterm: ESC [ 13 ; 2 ~
	return legacyShiftEnterRe.MatchString(payload)
}

func isCtrlJPayload(payload string) bool {
	if payload == "" {
		return false
	}
	payload = strings.TrimSuffix(payload, "~")
	// Kitty: ESC [ 10 ; <mods> u or ESC [ 106 ; <mods> u — ctrl bit is 4.
	if m := kittyCtrlJRe.FindStringSubmatch(payload); len(m) == 2 {
		mods, err := strconv.Atoi(m[1])
		if err == nil && mods&kittyModCtrl != 0 {
			return true
		}
	}
	// xterm modifyOtherKeys: ESC [ 27 ; 5 ; 10 ~
	if m := xtermCtrlJRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && mod == xtermModCtrl {
			return true
		}
	}
	return false
}

func isNewlineCSIPayload(payload string) bool {
	return isShiftEnterPayload(payload) || isCtrlJPayload(payload)
}

// isShiftEnterMsg reports newline CSI sequences for Shift+Enter across xterm
// modifyOtherKeys, Kitty keyboard protocol, and Ghostty keybind encodings.
func isShiftEnterMsg(msg tea.Msg) bool {
	if k, ok := msg.(tea.KeyPressMsg); ok {
		return isShiftEnterKeyMsg(k) || isLiteralNewlineKeyMsg(k)
	}
	return isShiftEnterPayload(csiPayload(msg))
}

func isNewlineInputMsg(msg tea.Msg) bool {
	if k, ok := msg.(tea.KeyPressMsg); ok {
		return isInputNewlineKey(k)
	}
	return isNewlineCSIPayload(csiPayload(msg))
}

func (m Model) maxInputHeight() int {
	if !m.ready || m.height <= 0 {
		return maxInputLines
	}
	footerH := lipgloss.Height(m.footerView())
	activityH := 0
	if m.showsActivity() {
		activityH = lipgloss.Height(m.activityView())
	}
	overlayH := 0
	switch {
	case m.commandPaletteActive():
		overlayH = m.commandPaletteHeight()
	case m.modelSelectorActive():
		overlayH = m.modelSelectorListHeight()
	}
	avail := m.height - footerH - activityH - overlayH - minViewportRows - inputChromeSlack
	return min(max(avail, 1), maxInputLines)
}

func (m Model) inputDisplayRows() int {
	val := m.input.Value()
	if val == "" {
		return 1
	}
	w := max(m.layout.InputWidth, 1)
	rows := 0
	for _, line := range strings.Split(val, "\n") {
		rows += wrappedInputRows(line, w)
	}
	return max(rows, 1)
}

func wrappedInputRows(line string, width int) int {
	if width < 1 {
		width = 1
	}
	if line == "" {
		return 1
	}
	wrapped := ansi.Hardwrap(ansi.Wordwrap(line, width, ""), width, false)
	return max(1, strings.Count(wrapped, "\n")+1)
}

func (m Model) desiredInputHeight() int {
	return min(m.inputDisplayRows(), m.maxInputHeight())
}

func (m Model) syncInputHeight() Model {
	h := m.desiredInputHeight()
	if m.input.Height() != h {
		m.input.SetHeight(h)
	}
	return m
}

func (m Model) inputCursorDisplayRow() int {
	w := max(m.layout.InputWidth, 1)
	lines := strings.Split(m.input.Value(), "\n")
	row := 0
	cur := m.input.Line()
	for i := 0; i < cur && i < len(lines); i++ {
		row += wrappedInputRows(lines[i], w)
	}
	row += m.input.LineInfo().RowOffset
	return row
}

// syncInputScroll mirrors bubbles textarea repositionView so the scrollbar
// thumb tracks the hidden lines above the input viewport.
func (m Model) syncInputScroll() Model {
	total := m.inputDisplayRows()
	visible := m.input.Height()
	if total <= visible {
		m.layout.InputScrollTop = 0
		return m
	}

	cursor := m.inputCursorDisplayRow()
	min := m.layout.InputScrollTop
	max := min + visible - 1
	if cursor < min {
		m.layout.InputScrollTop = cursor
	} else if cursor > max {
		m.layout.InputScrollTop = cursor - visible + 1
	}

	maxTop := total - visible
	if m.layout.InputScrollTop > maxTop {
		m.layout.InputScrollTop = maxTop
	}
	if m.layout.InputScrollTop < 0 {
		m.layout.InputScrollTop = 0
	}
	return m
}

func (m Model) syncInputChrome() Model {
	m = m.syncInputWidth()
	m = m.syncInputHeight()
	return m
}

// prepareInputHeightForNewline grows the textarea viewport before inserting a
// newline so the prior line stays visible (avoids stale YOffset scroll).
func (m Model) prepareInputHeightForNewline() Model {
	nextH := min(max(m.input.LineCount()+1, 1), m.maxInputHeight())
	if m.input.Height() < nextH {
		m.input.SetHeight(nextH)
	}
	return m
}

func (m Model) handleInputNewlineMsg(msg tea.Msg) (Model, tea.Cmd) {
	m = m.prepareInputHeightForNewline()
	var cmd tea.Cmd
	ctrlJ := tea.KeyPressMsg{Code: 'j', Mod: tea.ModCtrl}
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		if isLiteralNewlineKeyMsg(msg) {
			m.input, cmd = m.input.Update(msg)
		} else {
			m.input, cmd = m.input.Update(ctrlJ)
		}
	default:
		m.input, cmd = m.input.Update(ctrlJ)
	}
	m = m.syncInputWidth()
	if chromeH := m.chromeHeight(); chromeH != m.layout.ChromeH {
		m = m.syncLayout(m.content.AtBottom())
	}
	return m, cmd
}

func normalizeInputForSubmit(s string) string {
	if s == "" {
		return ""
	}
	lines := strings.Split(s, "\n")
	for i := range lines {
		lines[i] = strings.TrimRight(lines[i], " \t")
	}
	return strings.Trim(strings.Join(lines, "\n"), " \t\n")
}

func (m Model) resetInput() Model {
	m.input.SetValue("")
	m.input.SetHeight(1)
	m.layout.InputScrollTop = 0
	m.inputPendingEsc = false
	m.promptChar = ">"
	return m
}

func (m Model) finalizeInputEdit() (Model, tea.Cmd) {
	var cmds []tea.Cmd

	m = m.syncInputWidth()

	prevPrefix := m.showPromptPrefix
	m = m.syncPromptPrefix()
	if m.showPromptPrefix != prevPrefix {
		m = m.syncInputWidth()
	}

	prevSuggest := len(m.suggest.CmdSuggestions) + len(m.suggest.MentionSuggestions)
	var syncCmd tea.Cmd
	m, syncCmd = m.syncInputSuggestions()
	if syncCmd != nil {
		cmds = append(cmds, syncCmd)
	}
	if len(m.suggest.CmdSuggestions)+len(m.suggest.MentionSuggestions) != prevSuggest {
		m = m.syncLayout(m.content.AtBottom())
	}

	chromeH := m.chromeHeight()
	if chromeH != m.layout.ChromeH {
		m = m.syncLayout(m.content.AtBottom())
	}

	return m, tea.Batch(cmds...)
}

func isSlashCommand(s string) bool {
	return strings.HasPrefix(strings.TrimLeft(s, " \t"), "/")
}

func (m Model) handleSlashCommand(raw string) (Model, tea.Cmd, bool) {
	m = m.addUserMessage(strings.TrimSpace(raw))
	m = m.resetInput()

	result := command.Execute(raw, m.commandContext())
	if result.OpenModelSelector {
		m = m.openModelSelector(result.SelectorCatalog, result.SelectorQuery)
		return m, nil, true
	}
	m = m.applyModelSwitch(result.Switch)
	if result.Quit {
		m.quitting = true
		return m, tea.Sequence(disableTerminalFeatures(), tea.Quit), true
	}
	if output := strings.TrimSpace(result.Output); output != "" {
		m, cmd := m.withMessage(output)
		m = m.syncLayout(true)
		return m, cmd, true
	}
	m = m.syncLayout(true)
	return m, nil, true
}

func (m Model) trySubmitInput() (Model, tea.Cmd, bool) {
	if m.agent.Busy || m.shell.Running {
		return m, nil, false
	}
	val := normalizeInputForSubmit(m.input.Value())
	if val == "" {
		return m, nil, false
	}
	if val == ":q" || val == ":q!" {
		m.quitting = true
		return m, tea.Sequence(disableTerminalFeatures(), tea.Quit), true
	}
	if isSlashCommand(val) {
		return m.handleSlashCommand(val)
	}
	if strings.HasPrefix(strings.TrimLeft(val, " \t"), "!") {
		cmd, withContext, ok := parseShellCommand(val)
		if !ok {
			return m, nil, false
		}
		return m.handleShellSubmit(cmd, withContext)
	}
	val = stripTrigger(val)
	m = m.addUserMessage(val)
	m = m.resetInput()
	m = m.beginAgentTurn()
	m = m.syncLayout(true)
	var agentCmd tea.Cmd
	m, agentCmd = m.agentTurnCmds(val)
	return m, agentCmd, true
}
