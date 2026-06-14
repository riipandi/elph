package clipboardmedia

import (
	"sync"

	clip "golang.design/x/clipboard"
)

var (
	initOnce sync.Once
	initErr  error
)

// Init prepares the clipboard backend. Safe to call multiple times.
func Init() error {
	initOnce.Do(func() {
		initErr = clip.Init()
	})
	return initErr
}

// ReadImage returns PNG clipboard image bytes when available.
func ReadImage() ([]byte, bool) {
	if Init() != nil {
		return nil, false
	}
	data := clip.Read(clip.FmtImage)
	return data, len(data) > 0
}

// ReadText returns UTF-8 clipboard text when available.
func ReadText() (string, bool) {
	if Init() != nil {
		return "", false
	}
	data := clip.Read(clip.FmtText)
	if len(data) == 0 {
		return "", false
	}
	return string(data), true
}
