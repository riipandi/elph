package fetchurl

import (
	"strings"

	"github.com/microcosm-cc/bluemonday"
)

// stripPolicy removes all HTML markup (including script content) for plain-text output.
var stripPolicy = bluemonday.NewPolicy()

func htmlToText(data []byte) string {
	clean := stripPolicy.Sanitize(string(data))
	clean = strings.ReplaceAll(clean, "\r\n", "\n")
	clean = strings.ReplaceAll(clean, "\r", "\n")
	lines := strings.Split(clean, "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		out = append(out, line)
	}
	return strings.Join(out, "\n")
}

func isHTMLContentType(ct string) bool {
	ct = strings.ToLower(strings.TrimSpace(ct))
	return strings.Contains(ct, "text/html") || strings.Contains(ct, "application/xhtml")
}
