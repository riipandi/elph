package constants

import (
	"charm.land/lipgloss/v2"
	"charm.land/lipgloss/v2/compat"
)

// MessageKind identifies a stream message in the content area.
type MessageKind int

const (
	MessageUser MessageKind = iota
	MessageAI
	MessageSystem
	MessageTool
	MessageThinking
	MessageDetail
)

// Stream message colors — foreground + background only (no prefixes).
var (
	UserMsgFg = BrightText
	// UserMsgBg uses the pi-style green tint (aligned with DetailStatusSuccess).
	UserMsgBg = compat.AdaptiveColor{Light: lipgloss.Color("#E8F0E8"), Dark: lipgloss.Color("#283228")}

	AIMsgFg = BrightText
	AIMsgBg = lipgloss.NoColor{}

	SystemMsgFg = DimText
	SystemMsgBg = lipgloss.NoColor{}

	ToolMsgFg = compat.AdaptiveColor{Light: lipgloss.Color("#547DA7"), Dark: lipgloss.Color("#67E8F9")}
	ToolMsgBg = compat.AdaptiveColor{Light: lipgloss.Color("#E8E8F0"), Dark: lipgloss.Color("#0C1A1D")}

	ThinkingMsgFg = DimText
	ThinkingMsgBg = compat.AdaptiveColor{Light: lipgloss.Color("#EEEEEE"), Dark: lipgloss.Color("#232323")}
)

// MessageStyle returns the lipgloss style for a stream message kind.
func MessageStyle(kind MessageKind) lipgloss.Style {
	switch kind {
	case MessageUser:
		return lipgloss.NewStyle().Foreground(UserMsgFg).Background(UserMsgBg)
	case MessageAI:
		return lipgloss.NewStyle().Foreground(AIMsgFg).Background(AIMsgBg)
	case MessageSystem:
		return lipgloss.NewStyle().Foreground(SystemMsgFg).Background(SystemMsgBg)
	case MessageTool:
		return lipgloss.NewStyle().Foreground(ToolMsgFg).Background(ToolMsgBg)
	case MessageThinking:
		return lipgloss.NewStyle().Foreground(ThinkingMsgFg).Background(ThinkingMsgBg).Italic(true)
	case MessageDetail:
		return DetailStatusStyle(DetailStatusNeutral)
	default:
		return lipgloss.NewStyle().Foreground(AIMsgFg)
	}
}
