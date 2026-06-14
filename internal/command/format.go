package command

import (
	"strings"

	"github.com/riipandi/elph/internal/align"
)

// DisplayName returns the slash command id, including aliases when present.
func DisplayName(cmd SlashCommand) string {
	name := "/" + cmd.Name
	if len(cmd.Aliases) == 0 {
		return name
	}
	return name + " (" + strings.Join(cmd.Aliases, ", ") + ")"
}

// CommandID returns the slash command id shown in lists and the palette.
func CommandID(cmd SlashCommand) string {
	return "/" + cmd.Name
}

// PaletteID returns the slash command id shown in autocomplete, including argument hints.
func PaletteID(cmd SlashCommand) string {
	name := "/" + cmd.Name
	if hint := strings.TrimSpace(cmd.ArgumentHint); hint != "" {
		name += " " + hint
	}
	return name
}

// PaletteNameColumnWidth returns the display width of the widest palette command id.
func PaletteNameColumnWidth(commands []SlashCommand) int {
	names := make([]string, len(commands))
	for i, cmd := range commands {
		names[i] = PaletteID(cmd)
	}
	return align.ColumnWidth(names...)
}

// AlignedPaletteRow splits a command into a justified palette id and summary.
func AlignedPaletteRow(cmd SlashCommand, nameColW int) (name, gap, summary string) {
	return align.Row(PaletteID(cmd), nameColW, cmd.Description)
}

// NameColumnWidth returns the display width of the widest command id column.
func NameColumnWidth(commands []SlashCommand, includeAliases bool) int {
	names := make([]string, len(commands))
	for i, cmd := range commands {
		if includeAliases {
			names[i] = DisplayName(cmd)
		} else {
			names[i] = CommandID(cmd)
		}
	}
	return align.ColumnWidth(names...)
}

// AlignedRow splits a command into a justified command id and summary.
func AlignedRow(cmd SlashCommand, nameColW int, includeAliases bool) (name, gap, summary string) {
	if includeAliases {
		name = DisplayName(cmd)
	} else {
		name = CommandID(cmd)
	}
	return align.Row(name, nameColW, cmd.Description)
}

// FormatList renders commands as a justified two-column list.
func FormatList(commands []SlashCommand) string {
	if len(commands) == 0 {
		return ""
	}

	nameColW := NameColumnWidth(commands, true)
	var b strings.Builder
	for i, cmd := range commands {
		name, gap, summary := AlignedRow(cmd, nameColW, true)
		if i > 0 {
			b.WriteByte('\n')
		}
		b.WriteString("  ")
		b.WriteString(name)
		b.WriteString(gap)
		b.WriteString(summary)
	}
	return b.String()
}

// FormatHelp renders the full /help output.
func FormatHelp(commands []SlashCommand) string {
	var b strings.Builder
	b.WriteString("Available slash commands:\n\n")
	b.WriteString(FormatList(commands))
	return strings.TrimRight(b.String(), "\n")
}
