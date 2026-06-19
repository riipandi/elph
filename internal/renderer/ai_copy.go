package renderer

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"github.com/atotto/clipboard"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/uiconst"
)

const aiCopyHintText = "click or ctrl+y to copy"

func renderAIMessageFooter(blockWidth int, body string, showCopyHint bool) string {
	_ = blockWidth
	if !showCopyHint || strings.TrimSpace(body) == "" {
		return body
	}
	if strings.Contains(ansi.Strip(body), aiCopyHintText) {
		return body
	}
	_, hPad := messageBlockPadding(uiconst.MessageAI)
	return body + "\n\n" + dimItalicHintLine(hPad, aiCopyHintText)
}

func (m Model) lastAIMessageIndex() int {
	copyableIdx := -1
	for i := len(m.messages) - 1; i >= 0; i-- {
		msg := m.messages[i]
		if msg.kind == uiconst.MessageAI && strings.TrimSpace(msg.text) != "" {
			return i
		}
		// Fall back to last copyable detail (e.g. system prompt from /context)
		if copyableIdx < 0 && msg.copyable && strings.TrimSpace(msg.text) != "" {
			copyableIdx = i
		}
	}
	return copyableIdx
}

func (m Model) copyMessageAt(index int) (Model, tea.Cmd) {
	if index < 0 || index >= len(m.messages) {
		return m, nil
	}
	msg := m.messages[index]
	if (msg.kind != uiconst.MessageAI && !msg.copyable) || strings.TrimSpace(msg.text) == "" {
		return m, nil
	}
	_ = clipboard.WriteAll(msg.text)
	return m.withMessage("Copied to clipboard")
}

func (m Model) aiCopyFooterViewportY(msgIndex int) (int, bool) {
	target := -1
	m.walkContentLines(func(line int, ref contentLineRef) bool {
		if ref.messageIndex == msgIndex && ref.zone == zoneAICopyFooter {
			target = line
			return true
		}
		return false
	})
	if target < 0 {
		return -1, false
	}
	return m.viewportYForContentLine(target)
}

func (m Model) aiCopyAtViewportY(y int) (int, bool) {
	if !m.isInContentArea(y) {
		return -1, false
	}
	contentLine, ok := m.contentLineAtViewportY(y)
	if !ok {
		return -1, false
	}
	found := -1
	m.walkContentLines(func(line int, ref contentLineRef) bool {
		if line != contentLine {
			return false
		}
		if ref.zone == zoneAICopyFooter {
			found = ref.messageIndex
		}
		return true
	})
	if found < 0 {
		return -1, false
	}
	return found, true
}
