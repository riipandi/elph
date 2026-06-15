package renderer

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/clipboardmedia"
	"github.com/riipandi/elph/internal/mediaimage"
	"github.com/riipandi/elph/pkg/ai/provider"
)

type inputAttachment struct {
	AbsPath string
	RelPath string
	MIME    string
	Name    string
}

func (m Model) handlePasteKey() (Model, bool) {
	if !m.input.Focused() || m.agent.Busy || m.shell.Running || m.pasteEditorActive() {
		return m, false
	}

	if data, ok := clipboardmedia.ReadImage(); ok {
		if !m.modelSupportsImage {
			m, _ = m.withMessage("Current model does not support image input — switch to a vision model")
			return m, true
		}
		if len(m.pendingAttachments) >= mediaimage.MaxUserAttachments {
			m, _ = m.withMessage(fmt.Sprintf("At most %d images per message", mediaimage.MaxUserAttachments))
			return m, true
		}
		normalized, mime, _, _, err := mediaimage.Normalize(data, "image/png")
		if err != nil {
			m, _ = m.withMessage(fmt.Sprintf("Clipboard image: %v", err))
			return m, true
		}
		abs, rel, err := mediaimage.SaveAttachment(m.workDir, m.sessionID.Suffix(), normalized)
		if err != nil {
			m, _ = m.withMessage(fmt.Sprintf("Save image: %v", err))
			return m, true
		}
		m.pendingAttachments = append(m.pendingAttachments, inputAttachment{
			AbsPath: abs,
			RelPath: filepath.ToSlash(rel),
			MIME:    mime,
			Name:    filepath.Base(abs),
		})
		m.layout.ContentDirty = true
		m, _ = m.withMessage(fmt.Sprintf("Pasted image (%s)", filepath.Base(abs)))
		return m, true
	}

	if text, ok := clipboardmedia.ReadText(); ok && text != "" {
		return m.handlePasteContent(text)
	}
	return m, false
}

func (m Model) handlePasteContent(text string) (Model, bool) {
	text = strings.ReplaceAll(text, "\r\n", "\n")
	text = strings.ReplaceAll(text, "\r", "\n")
	if text == "" {
		return m, false
	}
	if !m.useRawPaste && shouldCollapsePaste(text) {
		m = m.insertCollapsedPaste(text)
	} else {
		m = m.insertTextAtCursor(text)
	}
	m = m.syncInputHeight()
	return m, true
}

func (m Model) attachmentsDisplaySuffix() string {
	if len(m.pendingAttachments) == 0 {
		return ""
	}
	names := make([]string, len(m.pendingAttachments))
	for i, att := range m.pendingAttachments {
		names[i] = att.Name
	}
	return "\n[images: " + strings.Join(names, ", ") + "]"
}

func (m Model) promptForSubmit(val string) string {
	prompt := strings.TrimSpace(val)
	if !m.modelSupportsImage {
		return prompt + runtimeMediaNote(m.pendingAttachments)
	}
	return prompt
}

func runtimeMediaNote(atts []inputAttachment) string {
	if len(atts) == 0 {
		return ""
	}
	paths := make([]string, len(atts))
	for i, att := range atts {
		paths[i] = att.RelPath
	}
	var b strings.Builder
	b.WriteString("\n\nAttached images (use ReadMediaFile to view):")
	for _, p := range paths {
		b.WriteString("\n- ")
		b.WriteString(p)
	}
	return b.String()
}

func (m Model) userImagesForTurn() []provider.ImageAttachment {
	if !m.modelSupportsImage || len(m.pendingAttachments) == 0 {
		return nil
	}
	out := make([]provider.ImageAttachment, 0, len(m.pendingAttachments))
	for _, att := range m.pendingAttachments {
		raw, err := os.ReadFile(att.AbsPath)
		if err != nil {
			continue
		}
		data, mime, _, _, err := mediaimage.Normalize(raw, att.MIME)
		if err != nil {
			continue
		}
		out = append(out, provider.ImageAttachment{MIME: mime, Data: data})
	}
	return out
}

func (m Model) clearPendingAttachments() Model {
	if len(m.pendingAttachments) == 0 {
		return m
	}
	for _, att := range m.pendingAttachments {
		_ = os.Remove(att.AbsPath)
	}
	m.pendingAttachments = nil
	m.layout.ContentDirty = true
	return m
}

func (m Model) removeLastAttachment() (Model, bool) {
	n := len(m.pendingAttachments)
	if n == 0 {
		return m, false
	}
	last := m.pendingAttachments[n-1]
	m.pendingAttachments = m.pendingAttachments[:n-1]
	_ = os.Remove(last.AbsPath)
	m.layout.ContentDirty = true
	return m, true
}

func (m Model) inputHasText() bool {
	return strings.TrimSpace(m.input.Value()) != ""
}

func (m Model) handleAttachmentRemoveMsg(msg tea.Msg) (Model, bool) {
	if !m.input.Focused() || m.agent.Busy || m.shell.Running || len(m.pendingAttachments) == 0 {
		return m, false
	}
	if m.inputHasText() {
		return m, false
	}

	if payload := csiPayload(msg); isMetaClearCSIPayload(payload) {
		m = m.clearPendingAttachments()
		return m, true
	}

	key, ok := msg.(tea.KeyPressMsg)
	if !ok {
		return m, false
	}
	switch {
	case isClearAttachmentsKey(key), isMetaClearAttachmentsKey(key):
		m = m.clearPendingAttachments()
		return m, true
	case isRemoveLastAttachmentKey(key), isCtrlRemoveLastAttachmentKey(key):
		m, _ = m.removeLastAttachment()
		return m, true
	default:
		return m, false
	}
}

func (m Model) attachmentHintView() string {
	if len(m.pendingAttachments) == 0 {
		return ""
	}
	var b strings.Builder
	b.WriteString(dimStyle.Render(fmt.Sprintf(
		"%d image(s) — Cmd/Ctrl+V add · Backspace/Ctrl+Del remove · Cmd+Del or Shift+Del clear",
		len(m.pendingAttachments),
	)))
	for _, att := range m.pendingAttachments {
		b.WriteString("\n")
		b.WriteString(dimStyle.Render("  • " + att.RelPath))
	}
	return b.String()
}
