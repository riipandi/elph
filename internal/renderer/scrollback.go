package renderer

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/x/ansi"
)

// scrollbackLineCount returns the number of terminal lines in scrollback content.
func scrollbackLineCount(content string) int {
	lines := strings.Split(strings.TrimSuffix(content, "\n"), "\n")
	if len(lines) == 1 && lines[0] == "" {
		return 0
	}
	return len(lines)
}

// scrollbackGeometry computes how many lines to clear and the cursor targets for
// a resize redraw. onScreenTotal is what is currently painted (old scrollback +
// old view). newTotal is the height after reflow (new scrollback + new view).
func scrollbackGeometry(onScreenTotal, newTotal int) (clearTotal int, cursorUp int) {
	clearTotal = max(onScreenTotal, newTotal)
	cursorUp = max(onScreenTotal-1, 0)
	return clearTotal, cursorUp
}

// redrawScrollback clears and repaints banner + message history in-place.
// Must run synchronously during Update, before Bubble Tea flushes the new view.
func redrawScrollback(oldScrollLines, oldViewLines, newViewLines, width int, content string) {
	lines := strings.Split(strings.TrimSuffix(content, "\n"), "\n")
	if len(lines) == 1 && lines[0] == "" {
		lines = nil
	}
	newScrollLines := len(lines)

	onScreenTotal := oldScrollLines + oldViewLines
	newTotal := newScrollLines + newViewLines
	clearTotal, cursorUp := scrollbackGeometry(onScreenTotal, newTotal)

	if cursorUp > 0 {
		fmt.Printf("\x1b[%dA", cursorUp)
	}
	for i := 0; i < clearTotal; i++ {
		fmt.Print("\x1b[2K\r")
		if i < clearTotal-1 {
			fmt.Print("\x1b[1B")
		}
	}
	if clearTotal > 1 {
		fmt.Printf("\x1b[%dA", clearTotal-1)
	}

	for i, line := range lines {
		printTerminalLine(line, width)
		if i < len(lines)-1 {
			fmt.Print("\r\n")
		}
	}

	// Park the cursor at the bottom of the view slot without writing blank lines.
	// Bubble Tea's next flush moves up (newViewLines-1) and paints input+footer.
	if newScrollLines > 0 {
		fmt.Print("\r\n")
	}
	if newViewLines > 1 {
		fmt.Printf("\x1b[%dB", newViewLines-1)
	}

	// Remove any stale content below the view slot (common when the block shrinks).
	fmt.Print(ansi.EraseScreenBelow)
}

func printTerminalLine(line string, width int) {
	if width > 0 {
		line = ansi.Truncate(line, width, "")
		if ansi.StringWidth(line) < width {
			line += ansi.EraseLineRight
		}
	}
	fmt.Print("\r", line)
}