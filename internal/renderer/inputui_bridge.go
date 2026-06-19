package renderer

import (
	"charm.land/bubbles/v2/textarea"
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/inputui"
)

type (
	inputAttachment  = inputui.Attachment
	pasteEditorState = inputui.PasteEditorState
)

const (
	maxInputLines    = inputui.MaxInputLines
	minViewportRows  = inputui.MinViewportRows
	inputChromeSlack = inputui.InputChromeSlack

	pasteCollapseMinLines = inputui.PasteCollapseMinLines
	pasteCollapseMinRunes = inputui.PasteCollapseMinRunes
)

var pasteTokenRe = inputui.PasteTokenRe

func inputContentWidth(outer int) int { return inputui.ContentWidth(outer) }
func overlayInputScrollBar(body, bar string, targetWidth int) string {
	return inputui.OverlayScrollBar(body, bar, targetWidth)
}
func wrappedInputRows(line string, width int) int { return inputui.WrappedRows(line, width) }
func pasteLineCount(text string) int              { return inputui.PasteLineCount(text) }
func shouldCollapsePaste(text string) bool        { return inputui.ShouldCollapsePaste(text) }
func pasteToken(id int) string                    { return inputui.PasteToken(id) }

//nolint:unused // kept for future use
func pasteDisplayToken(id int, lines int, pastes map[int]string) string {
	return inputui.PasteDisplayToken(id, lines, pastes)
}

func overlayInputPasteTokens(view, val string, pastes map[int]string) string {
	return inputui.OverlayPasteTokens(view, val, pastes)
}

func pasteDisplayValue(val string, pastes map[int]string) string {
	return inputui.DisplayValue(val, pastes)
}

func expandInputPastes(val string, pastes map[int]string) string {
	return inputui.ExpandPastes(val, pastes)
}

func pasteIDAtOffset(val string, offset int) (int, bool) {
	return inputui.PasteIDAtOffset(val, offset)
}
func pasteIDsInValue(val string) []int { return inputui.PasteIDsInValue(val) }
func pasteIDOnLine(val string, lineIdx int) (int, bool) {
	return inputui.PasteIDOnLine(val, lineIdx)
}
func normalizeInputForSubmit(s string) string { return inputui.NormalizeForSubmit(s) }
func isSlashCommand(s string) bool            { return inputui.IsSlashCommand(s) }

func activateTerminalFeaturesSync()    { inputui.ActivateTerminalFeaturesSync() }
func enableTerminalFeatures() tea.Cmd  { return inputui.EnableTerminalFeatures() }
func disableTerminalFeatures() tea.Cmd { return inputui.DisableTerminalFeatures() }

type termFeaturesMsg = inputui.TermFeaturesMsg

func csiPayload(msg tea.Msg) string           { return inputui.CSIPayload(msg) }
func csiPayloadFromString(text string) string { return inputui.CSIPayloadFromString(text) }

//nolint:unused // kept for future use
func rawCSIPayload(msg tea.Msg) (string, bool) {
	return inputui.RawCSIPayload(msg)
}
func isShiftEnterMsg(msg tea.Msg) bool           { return inputui.IsShiftEnterMsg(msg) }
func isNewlineInputMsg(msg tea.Msg) bool         { return inputui.IsNewlineInputMsg(msg) }
func isInputNewlineKey(msg tea.KeyPressMsg) bool { return inputui.IsInputNewlineKey(msg) }
func isShiftEnterKeyMsg(msg tea.KeyPressMsg) bool {
	return inputui.IsShiftEnterKeyMsg(msg)
}

func isLiteralNewlineKeyMsg(msg tea.KeyPressMsg) bool {
	return inputui.IsLiteralNewlineKeyMsg(msg)
}

//nolint:unused // kept for future use
func isShiftEnterPayload(payload string) bool {
	return inputui.IsShiftEnterPayload(payload)
}
func isCtrlJPayload(payload string) bool   { return inputui.IsCtrlJPayload(payload) }
func decodeCSIByteList(list string) string { return inputui.DecodeCSIByteList(list) }

const ctrlWCode = inputui.CtrlWCode

func wordDeleteMsgFromKey(msg tea.KeyPressMsg) (tea.KeyPressMsg, bool) {
	return inputui.WordDeleteMsgFromKey(msg)
}
func deleteWordBackwardKeyMsg() tea.KeyPressMsg { return inputui.DeleteWordBackwardKeyMsg() }
func deleteToEndKeyMsg() tea.KeyPressMsg        { return inputui.DeleteToEndKeyMsg() }

func configureInputKeyMap(ta *textarea.Model) { inputui.ConfigureKeyMap(ta) }
func isPasteKey(msg tea.KeyPressMsg) bool     { return inputui.IsPasteKey(msg) }
func isMetaClearCSIPayload(payload string) bool {
	return inputui.IsMetaClearCSIPayload(payload)
}

func isRemoveLastAttachmentKey(msg tea.KeyPressMsg) bool {
	return inputui.IsRemoveLastAttachmentKey(msg)
}

func isCtrlRemoveLastAttachmentKey(msg tea.KeyPressMsg) bool {
	return inputui.IsCtrlRemoveLastAttachmentKey(msg)
}

func isClearAttachmentsKey(msg tea.KeyPressMsg) bool {
	return inputui.IsClearAttachmentsKey(msg)
}

func isMetaClearAttachmentsKey(msg tea.KeyPressMsg) bool {
	return inputui.IsMetaClearAttachmentsKey(msg)
}
func isInputEscapeKey(msg tea.KeyPressMsg) bool { return inputui.IsInputEscapeKey(msg) }

//nolint:unused // kept for future use
func inputCursorByteCol(line string, col int) int {
	return inputui.CursorByteCol(line, col)
}

func runtimeMediaNote(atts []inputAttachment) string {
	return inputui.RuntimeMediaNote(atts)
}
