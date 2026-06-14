package constants

import (
	"strings"

	"github.com/mattn/go-runewidth"
)

// Braille mascot used in the TUI banner and CLI help.
const (
	LogoLine1 = "\u28FF\u28FF\u285F\u28FF\u285F\u28FF\u28FF"
	LogoLine2 = "\u28FF\u28FF\u28FF\u28FF\u28FF\u28FF\u28FF"
)

func Logo() string {
	return LogoLine1 + "\n" + LogoLine2
}

func LogoLines() []string {
	return []string{LogoLine1, LogoLine2}
}

// JoinSideBySide lays out two text blocks horizontally, top-aligned.
func JoinSideBySide(left, right []string, gap int) string {
	leftW := 0
	for _, line := range left {
		if w := runewidth.StringWidth(line); w > leftW {
			leftW = w
		}
	}

	rows := max(len(left), len(right))
	if rows == 0 {
		return ""
	}

	gapStr := strings.Repeat(" ", gap)
	var b strings.Builder
	for i := range rows {
		if i > 0 {
			b.WriteByte('\n')
		}

		var l, r string
		if i < len(left) {
			l = left[i]
		}
		if i < len(right) {
			r = right[i]
		}

		b.WriteString(l)
		if pad := leftW - runewidth.StringWidth(l); pad > 0 {
			b.WriteString(strings.Repeat(" ", pad))
		}
		b.WriteString(gapStr)
		b.WriteString(r)
	}
	return b.String()
}
