package renderer

import (
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/riipandi/elph/internal/runtime/toolresult"

	"charm.land/bubbles/v2/stopwatch"
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/internal/theme"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/riipandi/elph/pkg/core/agent"
)

// Pre-computed key-binding map for O(1) lookup on every keystroke.
var (
	keyActionMap   map[string]uiconst.KeyAction
	initKeyMapOnce sync.Once
)

func initKeyMap() {
	keyActionMap = make(map[string]uiconst.KeyAction, len(uiconst.DefaultKeyBindings))
	for _, kb := range uiconst.DefaultKeyBindings {
		if _, exists := keyActionMap[kb.Key]; !exists {
			keyActionMap[kb.Key] = kb.Action
		}
	}
}

func resolveKeyAction(msg tea.KeyPressMsg) uiconst.KeyAction {
	initKeyMapOnce.Do(initKeyMap)
	if action, ok := keyActionMap[msg.String()]; ok {
		return action
	}
	if action, ok := keyActionMap[msg.Keystroke()]; ok {
		return action
	}
	return ""
}

// ─── Update ──────────────────────────────────────────────────────────────────

func (m Model) handleAgentTurnClosed() (Model, tea.Cmd) {
	if m.agent.Busy {
		m.agent.Cancel = nil
		m.agent.Busy = false
		m.agent.Activity = agent.ActivityIdle
		m.agent.SpinnerFrame = 0
		m = m.stopActivityStopwatch()
		// Accumulate tool call count across turns
		m.toolCallCount += len(m.agent.TurnToolCalls)
	}
	m.agent.Events = nil
	// Reset stale todo/task state that may not have been cleaned up
	// if finishNativeToolCall was skipped (e.g. cancelled turn).
	m.agent.TodoListUpdating = false
	m.agent.TodoListBefore = nil
	m = m.syncLayout(true)
	if askCmd := m.markupAskUserCmd(); askCmd != nil {
		return m, askCmd
	}
	return m, nil
}

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	// Agent and layout messages must run while approval/ask dialogs are open.
	// Otherwise ToolCallStart and stream updates are swallowed by huh and never
	// reach the renderer (no tool detail box, no follow-up response stream).
	switch msg := msg.(type) {
	case agentEventMsg:
		return m.handleAgentEvent(msg)
	case agentTurnClosedMsg:
		return m.handleAgentTurnClosed()
	case markupAskUserCmdMsg:
		return m.handleMarkupAskUserCmd()
	case streamFlushMsg:
		return m.handleStreamFlush()
	}

	if m.toolInteractDialogActive() {
		return m.updateToolInteractForm(msg)
	}
	if m.modelsSyncDialogActive() {
		return m.updateModelsSyncForm(msg)
	}

	if paste, ok := msg.(tea.PasteMsg); ok {
		if m.pasteEditorActive() {
			var cmd tea.Cmd
			m.pasteEditor.Input, cmd = m.pasteEditor.Input.Update(paste)
			return m, cmd
		}
		if m.input.Focused() && !m.agent.Busy && !m.shell.Running {
			if updated, handled := m.handlePasteContent(paste.Content); handled {
				updatedM, finCmd := updated.finalizeInputEdit()
				return updatedM, finCmd
			}
		}
	}

	if m.pasteEditorActive() && isNewlineInputMsg(msg) {
		editorM, cmd := m.handlePasteEditorNewlineMsg(msg)
		return editorM, cmd
	}

	var cmds []tea.Cmd

	if key, ok := msg.(tea.KeyPressMsg); ok {
		if isToggleDetailKey(key) {
			var handled bool
			m, handled = m.handleToggleDetailKey()
			if handled {
				return m, nil
			}
		}
		if m.shell.Running && isShellCancelKey(key) {
			return m.cancelShell()
		}
		if m.agent.Busy && isShellCancelKey(key) {
			return m.cancelAgentTurn()
		}
		if m.modelSelectorActive() {
			var cmd tea.Cmd
			var handled bool
			m, cmd, handled = m.handleModelSelectorKey(key)
			if handled {
				if cmd != nil {
					cmds = append(cmds, cmd)
					return m, tea.Batch(cmds...)
				}
				return m, nil
			}
		}
		if m.pasteEditorActive() {
			var cmd tea.Cmd
			var handled bool
			m, cmd, handled = m.handlePasteEditorKey(key)
			if handled {
				pasteM, finCmd := m.finalizeInputEdit()
				if finCmd != nil {
					cmds = append(cmds, finCmd)
				}
				if cmd != nil {
					cmds = append(cmds, cmd)
				}
				return pasteM, tea.Batch(cmds...)
			}
		}
	}

	if m.input.Focused() && isNewlineInputMsg(msg) {
		newlineM, cmd := m.handleInputNewlineMsg(msg)
		return newlineM, cmd
	}

	if m.input.Focused() {
		if updated, handled := m.handleAttachmentRemoveMsg(msg); handled {
			attachM, finCmd := updated.finalizeInputEdit()
			return attachM, finCmd
		}
		if updated, handled := m.handleInputWordDelete(msg); handled {
			wordDelM, cmd := updated.finalizeInputEdit()
			return wordDelM, cmd
		}
	}

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		m.layout.ContentDirty = true
		m = m.syncLayout(false)

	case tea.BackgroundColorMsg:
		if m.themePreference == theme.Auto {
			m = m.applyResolvedTheme(msg.IsDark())
			m.layout.ContentDirty = true
			m = m.syncLayout(m.content.AtBottom())
		}

	case mouseReenableMsg:
		var cmd tea.Cmd
		m, cmd = m.resumeMouseAfterSelection()
		if cmd != nil {
			cmds = append(cmds, cmd)
		}

	case ctrlCResetMsg:
		m = m.cancelCtrlC()
		m.layout.ContentDirty = true
		m = m.syncLayout(false)

	case markdownRenderMsg:
		return m.handleMarkdownRenderMsg(msg)

	case toolInteractOfferMsg:
		var cmd tea.Cmd
		m, cmd = m.offerToolInteract(msg)
		m = m.syncLayout(true)
		if cmd != nil {
			return m, cmd
		}
		return m, nil

	case spinnerTickMsg, stopwatch.TickMsg, stopwatch.StartStopMsg, stopwatch.ResetMsg:
		var tickCmds []tea.Cmd
		m, tickCmds, _ = m.handleActivityTick(msg)
		cmds = append(cmds, tickCmds...)

	case shellOutputMsg:
		var cmd tea.Cmd
		m, cmd = m.handleShellOutput(msg)
		if cmd != nil {
			cmds = append(cmds, cmd)
		}
		return m, tea.Batch(cmds...)

	case shellOutputClosedMsg:
		// Output channel closed; shellDoneMsg follows.

	case shellDoneMsg:
		var cmd tea.Cmd
		m, cmd = m.finishShellDone(msg)
		if cmd != nil {
			cmds = append(cmds, cmd)
		}
		return m, tea.Batch(cmds...)

	case gitStatusMsg:
		m = m.handleGitStatus(msg)

	case gitRefreshTickMsg:
		cmds = append(cmds, refreshGitBranchCmd(m.workDir), gitRefreshTickCmd())

	case mentionIndexMsg:
		m.suggest.MentionIndexLoading = false
		if msg.workDir == m.workDir {
			m.suggest.MentionIndex = msg.entries
			m.suggest.MentionIndexDir = msg.workDir
		}
		var syncCmd tea.Cmd
		m, syncCmd = m.syncInputSuggestions()
		if syncCmd != nil {
			cmds = append(cmds, syncCmd)
		}

	case termFeaturesMsg:
		// Terminal feature setup complete.

	case modelsSyncOfferMsg:
		var cmd tea.Cmd
		m, cmd = m.offerModelsSync(msg.providers)
		if cmd != nil {
			cmds = append(cmds, cmd)
		}

	case modelsSyncCheckDoneMsg:
		if msg.err != nil {
			var cmd tea.Cmd
			m, cmd = m.withMessage(fmt.Sprintf("Model metadata check failed: %v", msg.err))
			if cmd != nil {
				cmds = append(cmds, cmd)
			}
		}

	case modelsSyncDoneMsg:
		m = m.finishModelsSync(msg)

	case tea.MouseWheelMsg:
		// Wheel always scrolls the viewport. Resume capture first if a text
		// selection just finished.
		if m.selectingText || !m.mouseEnabled {
			var cmd tea.Cmd
			m, cmd = m.resumeMouseAfterSelection()
			if cmd != nil {
				cmds = append(cmds, cmd)
			}
		}
		var cmd tea.Cmd
		m.content, cmd = m.content.Update(msg)
		cmds = append(cmds, cmd)
		return m, tea.Batch(cmds...)

	case tea.MouseMsg:
		mouse := msg.Mouse()

		var mouseCmds []tea.Cmd
		m, mouseCmds = m.handleMouse(msg)
		cmds = append(cmds, mouseCmds...)
		if m.selectingText {
			return m, tea.Batch(cmds...)
		}

		var cmd tea.Cmd
		if m.isInContentArea(mouse.Y) {
			m.content, cmd = m.content.Update(msg)
			cmds = append(cmds, cmd)
		}
		return m, tea.Batch(cmds...)

	case tea.KeyPressMsg:
		if m.selectingText {
			var cmd tea.Cmd
			m, cmd = m.resumeMouseAfterSelection()
			if cmd != nil {
				cmds = append(cmds, cmd)
			}
		}

		if m.input.Focused() && isInputNewlineKey(msg) {
			inputNewlineM, cmd := m.handleInputNewlineMsg(msg)
			return inputNewlineM, cmd
		}

		if m.input.Focused() {
			if isPasteKey(msg) {
				if updated, handled := m.handlePasteKey(); handled {
					pasteKeyM, finCmd := updated.finalizeInputEdit()
					return pasteKeyM, finCmd
				}
			}
			var paletteCmd tea.Cmd
			var consumed bool
			m, paletteCmd, consumed = m.handleInputPaletteKey(msg)
			if consumed {
				paletteM, finCmd := m.finalizeInputEdit()
				return paletteM, tea.Batch(paletteCmd, finCmd)
			}
		}

		if isContentScrollKey(msg) {
			var cmd tea.Cmd
			m.content, cmd = m.content.Update(msg)
			return m, cmd
		}

		action := resolveKeyAction(msg)

		switch action {
		case uiconst.ActionQuit:
			hasInput := m.input.Value() != ""

			if m.ctrlCPress == 1 && (hasInput || len(m.pendingAttachments) > 0) {
				m.ctrlCPress = 2
				m = m.resetInput()
				m = m.clearPendingAttachments()
				m = m.clearCtrlCNotice()
				m = m.syncLayout(true)
				return m, nil
			}

			if m.ctrlCPress == 2 || (m.ctrlCPress == 1 && !hasInput) {
				m.quitting = true
				return m, tea.Sequence(disableTerminalFeatures(), tea.Quit)
			}

			m.ctrlCPress = 1
			var cmd tea.Cmd
			m, cmd = m.withMessage("Press again to exit")
			m.ctrlCNoticeID = len(m.messages) - 1
			return m, tea.Batch(cmd, tea.Tick(doubleTapTimeout, func(t time.Time) tea.Msg {
				return ctrlCResetMsg{}
			}))

		case uiconst.ActionExit:
			// Let ctrl+d pass through to textarea when editing text (delete
			// character forward). Only exit when input is empty.
			if m.input.Focused() && m.input.Value() != "" {
				break
			}
			m.quitting = true
			return m, tea.Sequence(disableTerminalFeatures(), tea.Quit)
		case uiconst.ActionSwitchMode:
			m.mode = nextMode(m.mode)
			_ = settings.SetAgentMode(m.mode)
			switchModeM, cmd := m.withMessage(fmt.Sprintf("Switched to %s mode", m.mode))
			return switchModeM, cmd

		case uiconst.ActionCycleThink:
			cycleThinkM, cmd := m.cycleThinkingLevel()
			return cycleThinkM, cmd

		case uiconst.ActionCycleTheme:
			m.themePreference = theme.Next(m.themePreference)
			_ = settings.SetTheme(m.themePreference)
			m = m.applyResolvedTheme(theme.DetectTerminal())
			m.layout.ContentDirty = true
			m = m.syncLayout(m.content.AtBottom())
			cycleThemeM, cmd := m.withMessage(fmt.Sprintf("Theme: %s", m.themePreference))
			return cycleThemeM, cmd

		case uiconst.ActionSubmit:
			if isInputNewlineKey(msg) {
				break
			}
			if m.modelSelectorActive() {
				var cmd tea.Cmd
				var handled bool
				m, cmd, handled = m.confirmModelSelector()
				if handled {
					return m, cmd
				}
				break
			}
			if m.agent.Busy || m.shell.Running || !m.input.Focused() {
				break
			}
			var cmd tea.Cmd
			var ok bool
			m, cmd, ok = m.trySubmitInput()
			if ok {
				return m, cmd
			}

		case uiconst.ActionNewline:
			if !m.input.Focused() {
				break
			}
			actionNewlineM, cmd := m.handleInputNewlineMsg(msg)
			return actionNewlineM, cmd

		case uiconst.ActionCopy:
			if idx := m.lastAIMessageIndex(); idx >= 0 {
				return m.copyMessageAt(idx)
			}

		case uiconst.ActionOpenModelSelector:
			return m.triggerModelSelector()

		}

		m = m.cancelCtrlC()
	}

	var cmd tea.Cmd
	if !m.input.Focused() || !isInputEditingKey(msg) {
		m.content, cmd = m.content.Update(msg)
		cmds = append(cmds, cmd)
	}

	if m.input.Focused() && !m.pasteEditorActive() {
		m.input, cmd = m.input.Update(msg)
		cmds = append(cmds, cmd)
	}

	m, finCmd := m.finalizeInputEdit()
	if finCmd != nil {
		cmds = append(cmds, finCmd)
	}

	return m, tea.Batch(cmds...)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

func (m Model) addUserMessage(text string) Model {
	return m.addUserMessageAt(text, time.Now())
}

func (m Model) addUserMessageAt(text string, at time.Time) Model {
	if at.IsZero() {
		at = time.Now()
	}
	m.messages = append(m.messages, message{
		text:           text,
		kind:           uiconst.MessageUser,
		at:             at,
		detailExpanded: false,
	})
	m.session.AppendLog("user", text)
	m.layout.ContentDirty = true
	return m
}

func (m Model) addDetailMessage(label, body string) Model {
	return m.addDetailMessageAt(label, body, time.Now())
}

func (m Model) addDetailMessageAt(label, body string, at time.Time) Model {
	return m.addDetailMessageWithStatusAt(label, body, uiconst.DetailStatusNeutral, at)
}

func (m Model) addDetailMessageWithStatus(label, body string, status uiconst.DetailStatus) Model {
	return m.addDetailMessageWithStatusAt(label, body, status, time.Now())
}

func (m Model) addDetailMessageWithStatusAt(label, body string, status uiconst.DetailStatus, at time.Time) Model {
	if at.IsZero() {
		at = time.Now()
	}
	m.messages = append(m.messages, message{
		text:         body,
		kind:         uiconst.MessageDetail,
		detailLabel:  label,
		detailStatus: status,
		at:           at,
	})
	m.layout.ContentDirty = true
	return m
}

func (m Model) toggleDetailExpandAt(index int) (Model, bool) {
	if index < 0 || index >= len(m.messages) || !messageCollapsible(m.messages[index]) {
		return m, false
	}
	m.messages[index].detailExpanded = !m.messages[index].detailExpanded
	m.messages[index].renderCache = messageRenderCache{}
	m.layout.ContentDirty = true
	return m.clearStreamPrefixCache(), true
}

func (m Model) toggleLastDetailExpand() (Model, bool) {
	for i := len(m.messages) - 1; i >= 0; i-- {
		if !messageCollapsible(m.messages[i]) {
			continue
		}
		return m.toggleDetailExpandAt(i)
	}
	return m, false
}

func (m Model) addAIMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: uiconst.MessageAI})
	m.session.AppendLog("ai", text)
	m.layout.ContentDirty = true
	return m
}

func (m Model) addToolDetailMessage(toolName, body string) Model {
	return m.addToolDetailMessageWithStatus(toolName, body, uiconst.DetailStatusSuccess)
}

func (m Model) addToolDetailMessageWithStatus(toolName, body string, status uiconst.DetailStatus) Model {
	m = m.addDetailMessageWithStatusAt(toolName, body, status, time.Now())
	idx := len(m.messages) - 1
	m.messages[idx].detailExpanded = toolDetailExpandedByDefault(toolName, body)
	m.layout.ContentDirty = true
	return m
}

func (m Model) addToolDetailFromResult(toolName string, result toolresult.ToolResult) Model {
	body := toolresult.FormatToolDetailBodyFromResult(result)
	return m.addToolDetailMessageWithStatus(toolName, body, toolDetailStatus(result))
}

func (m Model) thinkingExpandedByDefault() bool {
	cfg, err := settings.Load()
	if err != nil {
		return false
	}
	return cfg.AutoExpandThinkingEnabled()
}

func (m Model) addThinkingMessage(text string) Model {
	m.messages = append(m.messages, message{
		text:           text,
		kind:           uiconst.MessageThinking,
		detailLabel:    "Thinking",
		detailExpanded: m.thinkingExpandedByDefault(),
	})
	m.layout.ContentDirty = true
	return m
}

func (m Model) withMessage(text string) (Model, tea.Cmd) {
	m.messages = append(m.messages, message{text: text, kind: uiconst.MessageSystem})
	m.session.AppendLog("system", text)
	m.layout.ContentDirty = true
	m = m.syncLayout(true)
	return m, nil
}

func (m Model) replaceNotice(text string) (Model, tea.Cmd) {
	newMsg := message{text: text, kind: uiconst.MessageSystem}
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages[m.ctrlCNoticeID] = newMsg
	} else {
		m.messages = append(m.messages, newMsg)
		m.ctrlCNoticeID = len(m.messages) - 1
	}
	m.layout.ContentDirty = true
	m = m.syncLayout(true)
	return m, nil
}

func (m Model) clearCtrlCNotice() Model {
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages = append(m.messages[:m.ctrlCNoticeID], m.messages[m.ctrlCNoticeID+1:]...)
		m.layout.ContentDirty = true
	}
	m.ctrlCNoticeID = -1
	return m
}

func (m Model) cancelCtrlC() Model {
	m.ctrlCPress = 0
	return m.clearCtrlCNotice()
}

func (m Model) syncPromptPrefix() Model {
	trimmed := strings.TrimLeft(m.input.Value(), " ")

	if trimmed == "" {
		m.promptChar = ">"
		return m
	}

	switch {
	case strings.HasPrefix(trimmed, "!!"):
		m.promptChar = "#"
	case strings.HasPrefix(trimmed, "!"):
		m.promptChar = "$"
	case strings.HasPrefix(trimmed, "/"):
		m.promptChar = "/"
	}

	return m
}

func stripTrigger(s string) string {
	s = strings.TrimLeft(s, " ")
	switch {
	case strings.HasPrefix(s, "!!"):
		return strings.TrimPrefix(s, "!!")
	case strings.HasPrefix(s, "!"):
		return strings.TrimPrefix(s, "!")
	case strings.HasPrefix(s, "/"):
		return strings.TrimPrefix(s, "/")
	}
	return s
}
