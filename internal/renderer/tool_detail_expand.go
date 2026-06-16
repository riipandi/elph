package renderer

// toolDetailExpandedByDefault chooses the initial expand state for native tool detail boxes.
// Only running placeholders stay expanded (so streaming output is visible).
// All completed tool results start collapsed regardless of label or content length.
// User shell commands (!/!!) are always expanded — set independently via addShellDetailMessageAt.
func toolDetailExpandedByDefault(_ string, body string) bool {
	return isRunningDetailPlaceholder(body)
}
