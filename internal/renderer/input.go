package renderer

import (
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/command"
	"github.com/riipandi/elph/internal/inputui"
)

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
	case m.modelsSyncDialogActive():
		overlayH = m.modelsSyncDialogHeight()
	case m.commandPaletteActive():
		overlayH = m.commandPaletteHeight()
	case m.modelSelectorActive():
		overlayH = m.modelSelectorListHeight()
	case m.pasteEditorActive():
		overlayH = m.pasteEditorHeight()
	}
	avail := m.height - footerH - activityH - m.todoPanelHeight() - m.toolInteractDialogHeight() - overlayH - minViewportRows - inputChromeSlack
	return min(max(avail, 1), maxInputLines)
}

func (m Model) inputDisplayRows() int {
	return inputui.DisplayRows(m.input.Value(), m.inputPastes, m.layout.InputWidth)
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

func (m Model) resetInput() Model {
	m.input.SetValue("")
	m.input.SetHeight(1)
	m.layout.InputScrollTop = 0
	m.inputPendingEsc = false
	m.promptChar = ">"
	return m.clearInputPastes()
}

func (m Model) finalizeInputEdit() (Model, tea.Cmd) {
	var cmds []tea.Cmd

	m = m.syncInputWidth()

	prevPrefix := m.showPromptPrefix
	m = m.syncPromptPrefix()
	if m.showPromptPrefix != prevPrefix {
		m = m.syncInputWidth()
	}

	m = m.pruneInputPastes()

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

func (m Model) handleSlashCommand(raw string) (Model, tea.Cmd, bool) {
	trimmed := strings.TrimSpace(raw)
	m = m.ensurePromptTemplates()
	result := command.Execute(raw, m.commandContext())
	if result.OpenModelSelector {
		m = m.openModelSelector(result.SelectorCatalog, result.SelectorQuery)
		return m, nil, true
	}
	m = m.applyModelSwitch(result.Switch)
	// Save the original system prompt before /commit overrides it, so it can be
	// restored after the agent turn completes.
	savedPrompt := m.session.SystemPrompt
	// Apply system prompt overrides before starting the agent turn.
	// ClearSystemPrompt removes the project system prompt to save tokens;
	// SystemPrompt (when non-empty) replaces it with a minimal, focused prompt.
	if result.ClearSystemPrompt {
		if result.SystemPrompt != "" {
			m.session.SystemPrompt = result.SystemPrompt
		} else {
			m.session.SystemPrompt = ""
		}
	} else if result.SystemPrompt != "" {
		m.session.SystemPrompt = result.SystemPrompt
	}
	if result.CommitAfterTurn {
		m.agent.SavedSystemPrompt = savedPrompt
	}
	if prompt := strings.TrimSpace(result.AgentPrompt); prompt != "" {
		if !m.hasActiveModel() {
			inputUpdated, cmd := m.promptSelectModel()
			return inputUpdated, cmd, true
		}
		at := time.Now()
		m.agent.ResolvedAskUsers = nil
		m = m.addUserMessageAt(trimmed, at)
		detailLabel := strings.TrimSpace(result.DetailLabel)
		detailBody := strings.TrimSpace(result.DetailBody)
		if detailLabel != "" && detailBody != "" {
			m = m.addDetailMessageAt(detailLabel, detailBody, at)
			if result.DetailExpanded {
				m.messages[len(m.messages)-1].detailExpanded = true
				m.layout.ContentDirty = true
			}
			m.session.AppendLog("detail", detailLabel)
		} else {
			m = m.addDetailMessageAt("Prompt", prompt, at)
			m.session.AppendLog("prompt", prompt)
		}
		m = m.resetInput()
		m = m.beginAgentTurn()
		m = m.syncLayout(true)
		var agentCmd tea.Cmd
		m, agentCmd = m.agentTurnCmds(prompt, nil)
		if result.CommitAfterTurn {
			m.agent.CommitWorkDir = m.workDir
		}
		return m, agentCmd, true
	}
	m = m.addUserMessage(trimmed)
	m = m.resetInput()

	if result.Quit {
		m.quitting = true
		return m, tea.Sequence(disableTerminalFeatures(), tea.Quit), true
	}
	if result.CompactHistory {
		return m.handleCompactHistory(result), nil, true
	}
	if result.ContextUsage {
		m = m.handleContextUsage()
		m = m.syncLayout(true)
		return m, nil, true
	}
	if label := strings.TrimSpace(result.DetailLabel); label != "" && strings.TrimSpace(result.DetailBody) != "" {
		at := time.Now()
		m = m.addDetailMessageAt(label, result.DetailBody, at)
		if result.DetailExpanded {
			m.messages[len(m.messages)-1].detailExpanded = true
			m.layout.ContentDirty = true
		}
		m.session.AppendLog("detail", label)
		m = m.syncLayout(true)
		return m, nil, true
	}
	if output := strings.TrimSpace(result.Output); output != "" {
		msgM, cmd := m.withMessage(output)
		msgM = msgM.syncLayout(true)
		return msgM, cmd, true
	}
	m = m.syncLayout(true)
	return m, nil, true
}

func (m Model) trySubmitInput() (Model, tea.Cmd, bool) {
	if m.agent.Busy || m.shell.Running {
		return m, nil, false
	}
	val := normalizeInputForSubmit(expandInputPastes(m.input.Value(), m.inputPastes))
	if val == "" && len(m.pendingAttachments) == 0 {
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
	if !m.hasActiveModel() {
		updatedMsg, cmd := m.promptSelectModel()
		return updatedMsg, cmd, true
	}
	val = stripTrigger(val)
	display := strings.TrimSpace(val)
	if suffix := inputui.DisplaySuffix(m.pendingAttachments); suffix != "" {
		if display == "" {
			display = strings.TrimPrefix(suffix, "\n")
		} else {
			display += suffix
		}
	}
	userImages := m.userImagesForTurn()
	prompt := m.promptForSubmit(val)
	m.agent.ResolvedAskUsers = nil
	m = m.addUserMessage(display)
	m = m.clearPendingAttachments()
	m = m.resetInput()
	m = m.beginAgentTurn()
	m = m.syncLayout(true)
	var agentCmd tea.Cmd
	m, agentCmd = m.agentTurnCmds(prompt, userImages)
	return m, agentCmd, true
}
