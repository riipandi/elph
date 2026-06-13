package renderer

import (
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
)

const (
	scrollBarWidth     = 1
	messageScrollInset = 1 // extra inset so message backgrounds stay clear of the bar
)

var (
	scrollTrackStyle = lipgloss.NewStyle().Foreground(constants.DimText)
	scrollThumbStyle = lipgloss.NewStyle().Foreground(constants.Gray)
)

func (m Model) contentScrollable() bool {
	if !m.ready || m.content.Height() <= 0 {
		return false
	}
	return m.content.TotalLineCount() > m.content.Height()
}

// contentAreaWidth is the rendered width of stream message backgrounds and the
// scrollable content column. Uses the settled viewport width so message blocks
// never extend under the scrollbar gutter.
func (m Model) contentAreaWidth() int {
	if m.content.Width() > 0 {
		return m.content.Width()
	}
	return max(m.width, 1)
}

func (m Model) targetContentWidth() int {
	if m.contentScrollable() {
		return max(m.width-scrollBarWidth, 1)
	}
	return m.width
}

// messageAreaWidth is the rendered width for stream message backgrounds.
// Slightly narrower than the viewport when scrollable so blocks do not hug the bar.
func (m Model) messageAreaWidth() int {
	w := m.contentAreaWidth()
	if m.contentScrollable() {
		w -= messageScrollInset
	}
	return max(w, 1)
}

// chromeOuterWidth is the edge-to-edge width shared by the banner and input box.
func (m Model) chromeOuterWidth() int {
	return m.contentAreaWidth()
}

// borderedChromeWidth is the lipgloss Width for rounded-border chrome.
// In Lip Gloss v2, Style.Width sets the total rendered width including border.
func borderedChromeWidth(outer int) int {
	return max(outer, 1)
}

func scrollBarFor(viewportH, total, scrollTop int) string {
	if viewportH <= 0 || total <= viewportH {
		return ""
	}

	thumbH := max(1, (viewportH*viewportH)/total)
	if thumbH > viewportH {
		thumbH = viewportH
	}

	maxTop := total - viewportH
	if scrollTop < 0 {
		scrollTop = 0
	}
	if scrollTop > maxTop {
		scrollTop = maxTop
	}

	var thumbStart int
	if maxTop > 0 {
		thumbStart = int(float64(scrollTop) / float64(maxTop) * float64(viewportH-thumbH))
	}
	if thumbStart < 0 {
		thumbStart = 0
	}
	if thumbStart > viewportH-thumbH {
		thumbStart = viewportH - thumbH
	}

	lines := make([]string, viewportH)
	for i := range viewportH {
		if i >= thumbStart && i < thumbStart+thumbH {
			lines[i] = scrollThumbStyle.Render("█")
		} else {
			lines[i] = scrollTrackStyle.Render("░")
		}
	}
	return strings.Join(lines, "\n")
}

func (m Model) contentAreaView() string {
	vp := m.content.View()
	if !m.contentScrollable() {
		return lipgloss.NewStyle().Width(m.width).MaxWidth(m.width).Render(vp)
	}

	vpBox := lipgloss.NewStyle().Width(m.content.Width()).MaxWidth(m.content.Width()).Render(vp)
	row := lipgloss.JoinHorizontal(lipgloss.Top, vpBox, m.contentScrollBarView())
	return lipgloss.NewStyle().Width(m.width).MaxWidth(m.width).Render(row)
}

func (m Model) contentScrollBarView() string {
	return scrollBarFor(m.content.Height(), m.content.TotalLineCount(), m.content.YOffset())
}

func (m Model) inputScrollable() bool {
	return m.inputDisplayRows() > m.input.Height()
}

func (m Model) inputScrollBarView() string {
	return scrollBarFor(m.input.Height(), m.inputDisplayRows(), m.layout.InputScrollTop)
}
