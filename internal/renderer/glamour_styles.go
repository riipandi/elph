package renderer

import (
	"charm.land/glamour/v2/ansi"
	"charm.land/glamour/v2/styles"
	"github.com/riipandi/elph/internal/theme"
)

func elphGlamourStyleConfig() ansi.StyleConfig {
	cfg := styles.LightStyleConfig
	if theme.IsDark() {
		cfg = styles.DarkStyleConfig
	}
	// Match H1 to other headings (default dark style gives H1 a distinct badge).
	h1 := cfg.H2
	h1.Prefix = "# "
	h1.BackgroundColor = nil
	h1.Color = nil
	h1.Bold = nil
	cfg.H1 = h1
	cfg.ImageText.Format = "{{.text}}"
	return cfg
}
