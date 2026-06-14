package renderer

import (
	"fmt"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/stretchr/testify/require"
)

func TestSubmitCapturesImagesBeforeClear(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "paste.png")
	writeTestPNGFile(t, path, 16, 16)

	m := testModel()
	m.workDir = dir
	m.modelSupportsImage = true
	m.pendingAttachments = []inputAttachment{{
		AbsPath: path,
		RelPath: "paste.png",
		MIME:    "image/png",
		Name:    "paste.png",
	}}

	images := m.userImagesForTurn()
	require.Len(t, images, 1)
	require.NotEmpty(t, images[0].Data)

	m = m.clearPendingAttachments()
	require.Empty(t, m.pendingAttachments)

	opts := m.buildTurnOptions("describe this", images, nil)
	require.Len(t, opts.UserImages, 1)
}

func TestRemoveLastAttachment(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "a.png")
	writeTestPNGFile(t, path, 8, 8)

	m := testModel()
	m.pendingAttachments = []inputAttachment{{
		AbsPath: path,
		RelPath: "a.png",
		MIME:    "image/png",
		Name:    "a.png",
	}}

	m, ok := m.removeLastAttachment()
	require.True(t, ok)
	require.Empty(t, m.pendingAttachments)
	_, err := os.Stat(path)
	require.True(t, os.IsNotExist(err))
}

func TestHandleAttachmentRemoveKeyRequiresEmptyInput(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "a.png")
	writeTestPNGFile(t, path, 8, 8)

	m := testModel()
	m.input.SetValue("keep")
	m.pendingAttachments = []inputAttachment{{AbsPath: path, Name: "a.png"}}

	updated, handled := m.handleAttachmentRemoveMsg(keyCtrl('v'))
	require.False(t, handled)
	require.Len(t, updated.pendingAttachments, 1)

	m.input.SetValue("")
	updated, handled = m.handleAttachmentRemoveMsg(tea.KeyPressMsg{Code: tea.KeyBackspace})
	require.True(t, handled)
	require.Empty(t, updated.pendingAttachments)
}

func TestCtrlDeleteRemovesLastAttachment(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "a.png")
	writeTestPNGFile(t, path, 8, 8)

	m := testModel()
	m.pendingAttachments = []inputAttachment{{AbsPath: path, Name: "a.png"}}

	updated, handled := m.handleAttachmentRemoveMsg(keyCtrlDelete())
	require.True(t, handled)
	require.Empty(t, updated.pendingAttachments)
}

func TestMetaDeleteClearsAllAttachments(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	paths := make([]string, 2)
	for i := range paths {
		paths[i] = filepath.Join(dir, fmt.Sprintf("%d.png", i))
		writeTestPNGFile(t, paths[i], 8, 8)
	}

	m := testModel()
	m.pendingAttachments = []inputAttachment{
		{AbsPath: paths[0], Name: "0.png"},
		{AbsPath: paths[1], Name: "1.png"},
	}

	updated, handled := m.handleAttachmentRemoveMsg(keyMetaDelete())
	require.True(t, handled)
	require.Empty(t, updated.pendingAttachments)
}

func TestGhosttyCmdDeleteCSIClearsAttachments(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "a.png")
	writeTestPNGFile(t, path, 8, 8)

	m := testModel()
	m.pendingAttachments = []inputAttachment{{AbsPath: path, Name: "a.png"}}

	updated, handled := m.handleAttachmentRemoveMsg(rawCSIMsg([]byte("\x1b[3;9~")))
	require.True(t, handled)
	require.Empty(t, updated.pendingAttachments)
}

func TestUpdateGhosttyCmdDeleteCSIClearsAttachmentsBeforeWordDelete(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "a.png")
	writeTestPNGFile(t, path, 8, 8)

	m := testModel()
	m.pendingAttachments = []inputAttachment{{AbsPath: path, Name: "a.png"}}

	updated, _ := m.Update(rawCSIMsg([]byte("\x1b[3;9~")))
	m = updated.(Model)
	require.Empty(t, m.pendingAttachments)
}

func TestAttachmentRemoveIgnoredWhenInputHasText(t *testing.T) {
	t.Parallel()

	m := testModel()
	m.input.SetValue("text")
	m.pendingAttachments = []inputAttachment{{Name: "a.png"}}

	updated, handled := m.handleAttachmentRemoveMsg(keyMetaDelete())
	require.False(t, handled)
	require.Len(t, updated.pendingAttachments, 1)
}

func TestIsPasteKeyMacAndCtrl(t *testing.T) {
	t.Parallel()

	require.True(t, isPasteKey(keyCtrl('v')))
	require.True(t, isPasteKey(keyMeta('v')))
	require.False(t, isPasteKey(keyRune('v')))
}

func writeTestPNGFile(t *testing.T, path string, w, h int) {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	f, err := os.Create(path)
	require.NoError(t, err)
	defer f.Close()
	require.NoError(t, png.Encode(f, img))
}
