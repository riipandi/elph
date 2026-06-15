package renderer

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"
)

const (
	pasteCollapseMinLines = 4
	pasteCollapseMinRunes = 400
)

var pasteTokenRe = regexp.MustCompile(`\[\[paste:(\d+)\]\]`)

func pasteLineCount(text string) int {
	if text == "" {
		return 0
	}
	return strings.Count(text, "\n") + 1
}

func shouldCollapsePaste(text string) bool {
	if pasteLineCount(text) >= pasteCollapseMinLines {
		return true
	}
	return len([]rune(text)) >= pasteCollapseMinRunes
}

func pasteToken(id int) string {
	return fmt.Sprintf("[[paste:%d]]", id)
}

func pasteDisplayToken(id int, lines int, pastes map[int]string) string {
	if pastes != nil {
		if text, ok := pastes[id]; ok {
			lines = pasteLineCount(text)
		}
	}
	return fmt.Sprintf("[Pasted: %d lines]", lines)
}

func overlayInputPasteTokens(view, val string, pastes map[int]string) string {
	if len(pastes) == 0 || view == "" {
		return view
	}
	out := view
	for _, loc := range pasteTokenRe.FindAllStringSubmatchIndex(val, -1) {
		if len(loc) < 4 {
			continue
		}
		token := val[loc[0]:loc[1]]
		id, err := strconv.Atoi(val[loc[2]:loc[3]])
		if err != nil {
			continue
		}
		display := pasteDisplayToken(id, 0, pastes)
		out = strings.ReplaceAll(out, token, display)
	}
	return out
}

func pasteDisplayValue(val string, pastes map[int]string) string {
	return pasteTokenRe.ReplaceAllStringFunc(val, func(match string) string {
		sub := pasteTokenRe.FindStringSubmatch(match)
		if len(sub) < 2 {
			return match
		}
		id, err := strconv.Atoi(sub[1])
		if err != nil {
			return match
		}
		return pasteDisplayToken(id, 0, pastes)
	})
}

func expandInputPastes(val string, pastes map[int]string) string {
	return pasteTokenRe.ReplaceAllStringFunc(val, func(match string) string {
		sub := pasteTokenRe.FindStringSubmatch(match)
		if len(sub) < 2 {
			return match
		}
		id, err := strconv.Atoi(sub[1])
		if err != nil {
			return match
		}
		if text, ok := pastes[id]; ok {
			return text
		}
		return match
	})
}

func pasteIDAtOffset(val string, offset int) (int, bool) {
	for _, loc := range pasteTokenRe.FindAllStringSubmatchIndex(val, -1) {
		if len(loc) < 4 {
			continue
		}
		start, end := loc[0], loc[1]
		if offset >= start && offset <= end {
			id, err := strconv.Atoi(val[loc[2]:loc[3]])
			if err != nil {
				return 0, false
			}
			return id, true
		}
	}
	return 0, false
}

func pasteIDsInValue(val string) []int {
	var ids []int
	for _, loc := range pasteTokenRe.FindAllStringSubmatchIndex(val, -1) {
		if len(loc) < 4 {
			continue
		}
		id, err := strconv.Atoi(val[loc[2]:loc[3]])
		if err != nil {
			continue
		}
		ids = append(ids, id)
	}
	return ids
}

func pasteIDOnLine(val string, lineIdx int) (int, bool) {
	lines := strings.Split(val, "\n")
	if lineIdx < 0 || lineIdx >= len(lines) {
		return 0, false
	}
	sub := pasteTokenRe.FindStringSubmatch(lines[lineIdx])
	if len(sub) < 2 {
		return 0, false
	}
	id, err := strconv.Atoi(sub[1])
	return id, err == nil
}

func (m Model) pasteIDForEdit() (int, bool) {
	val := m.input.Value()
	if id, ok := pasteIDAtOffset(val, m.inputCursorOffset()); ok {
		if _, ok := m.inputPastes[id]; ok {
			return id, true
		}
	}
	if id, ok := pasteIDOnLine(val, m.input.Line()); ok {
		if _, ok := m.inputPastes[id]; ok {
			return id, true
		}
	}
	ids := pasteIDsInValue(val)
	if len(ids) == 1 {
		if _, ok := m.inputPastes[ids[0]]; ok {
			return ids[0], true
		}
	}
	return 0, false
}

func (m Model) restoreInputCursorLineCol(line, col int) Model {
	lines := strings.Split(m.input.Value(), "\n")
	if len(lines) == 0 {
		m.input.MoveToBegin()
		return m
	}
	if line < 0 {
		line = 0
	}
	if line >= len(lines) {
		line = len(lines) - 1
	}
	maxCol := len([]rune(lines[line]))
	if col > maxCol {
		col = maxCol
	}
	if col < 0 {
		col = 0
	}

	m.input.MoveToBegin()
	for m.input.Line() < line {
		m.input.CursorDown()
	}
	m.input.SetCursorColumn(col)
	return m
}

func (m Model) setInputCursorByteOffset(off int) Model {
	val := m.input.Value()
	if len(val) == 0 {
		return m
	}
	off = max(0, min(off, len(val)))
	targetLine := strings.Count(val[:off], "\n")
	lineStart := strings.LastIndex(val[:off], "\n") + 1
	targetCol := len([]rune(val[lineStart:off]))
	return m.restoreInputCursorLineCol(targetLine, targetCol)
}

func (m Model) placeCursorOnPasteToken(id int) Model {
	token := pasteToken(id)
	idx := strings.Index(m.input.Value(), token)
	if idx < 0 {
		return m
	}
	return m.setInputCursorByteOffset(idx + len(token))
}

func (m Model) pruneInputPastes() Model {
	if len(m.inputPastes) == 0 {
		return m
	}
	val := m.input.Value()
	seen := make(map[int]struct{})
	for _, loc := range pasteTokenRe.FindAllStringSubmatchIndex(val, -1) {
		if len(loc) < 4 {
			continue
		}
		id, err := strconv.Atoi(val[loc[2]:loc[3]])
		if err != nil {
			continue
		}
		seen[id] = struct{}{}
	}
	for id := range m.inputPastes {
		if _, ok := seen[id]; !ok {
			delete(m.inputPastes, id)
		}
	}
	return m
}

func (m Model) insertTextAtCursor(text string) Model {
	val := m.input.Value()
	offset := m.inputCursorOffset()
	if offset < 0 {
		offset = 0
	}
	if offset > len(val) {
		offset = len(val)
	}
	m.input.SetValue(val[:offset] + text + val[offset:])
	return m
}

func (m Model) insertCollapsedPaste(text string) Model {
	if m.inputPastes == nil {
		m.inputPastes = make(map[int]string)
	}
	id := m.nextPasteID
	m.nextPasteID++
	m.inputPastes[id] = text
	token := pasteToken(id)
	m = m.insertTextAtCursor(token)
	return m.placeCursorOnPasteToken(id)
}

func (m Model) replacePasteToken(id int, text string) Model {
	token := pasteToken(id)
	val := m.input.Value()
	replaced := false
	out := pasteTokenRe.ReplaceAllStringFunc(val, func(match string) string {
		sub := pasteTokenRe.FindStringSubmatch(match)
		if len(sub) < 2 {
			return match
		}
		matchID, err := strconv.Atoi(sub[1])
		if err != nil || matchID != id {
			return match
		}
		replaced = true
		return token
	})
	if replaced {
		m.input.SetValue(out)
	}
	return m
}

func (m Model) clearInputPastes() Model {
	m.inputPastes = nil
	m.nextPasteID = 0
	m.pasteEditor = pasteEditorState{}
	return m
}

func (m Model) pasteHintView() string {
	if m.pasteEditorActive() {
		return ""
	}
	id, ok := m.pasteIDForEdit()
	if !ok {
		return ""
	}
	lines := pasteLineCount(m.inputPastes[id])
	return dimStyle.Render(fmt.Sprintf("Pasted block · %d lines · ctrl+o to preview/edit", lines))
}
