package renderer

import (
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
)

var modeList = []constants.AgentMode{
	constants.ModeBuild,
	constants.ModePlan,
	constants.ModeAsk,
	constants.ModeBrave,
}

var (
	inputBorderByMode             = make(map[constants.AgentMode]lipgloss.Style, len(modeList))
	inputBorderAttachedByMode     = make(map[constants.AgentMode]lipgloss.Style, len(modeList))
	paletteBorderByMode           = make(map[constants.AgentMode]lipgloss.Style, len(modeList))
	modelSelectorListBorderByMode = make(map[constants.AgentMode]lipgloss.Style, len(modeList))
)

func init() {
	for _, mode := range modeList {
		color := constants.ModeBorderColor(mode)
		inputBorderByMode[mode] = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(color).
			Padding(0, 1)
		inputBorderAttachedByMode[mode] = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(color).
			BorderTop(false).
			Padding(0, 1)
		paletteBorderByMode[mode] = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(color).
			BorderBottom(false).
			Padding(0, 1)
		modelSelectorListBorderByMode[mode] = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(color).
			BorderBottom(false).
			Padding(0, 1).
			PaddingBottom(1)
	}
}

func cachedInputBorder(mode constants.AgentMode) lipgloss.Style {
	if style, ok := inputBorderByMode[mode]; ok {
		return style
	}
	return inputBorderByMode[constants.ModeBuild]
}

func cachedInputBorderAttached(mode constants.AgentMode) lipgloss.Style {
	if style, ok := inputBorderAttachedByMode[mode]; ok {
		return style
	}
	return inputBorderAttachedByMode[constants.ModeBuild]
}

func paletteBorder(mode constants.AgentMode) lipgloss.Style {
	if style, ok := paletteBorderByMode[mode]; ok {
		return style
	}
	return paletteBorderByMode[constants.ModeBuild]
}

func modelSelectorListBorder(mode constants.AgentMode) lipgloss.Style {
	if style, ok := modelSelectorListBorderByMode[mode]; ok {
		return style
	}
	return modelSelectorListBorderByMode[constants.ModeBuild]
}
