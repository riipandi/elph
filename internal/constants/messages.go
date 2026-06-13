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
)

// Stream message colors — foreground + background only (no prefixes).
var (
	UserMsgFg = BrightText
	UserMsgBg = compat.AdaptiveColor{Light: lipgloss.Color("#F3F4F6"), Dark: lipgloss.Color("#2A2A2A")}

	AIMsgFg = BrightText
	AIMsgBg = lipgloss.NoColor{}

	SystemMsgFg = DimText
	SystemMsgBg = lipgloss.NoColor{}

	ToolMsgFg = compat.AdaptiveColor{Light: lipgloss.Color("#0E7490"), Dark: lipgloss.Color("#67E8F9")}
	ToolMsgBg = compat.AdaptiveColor{Light: lipgloss.Color("#ECFEFF"), Dark: lipgloss.Color("#0C1A1D")}

	ThinkingMsgFg = DimText
	ThinkingMsgBg = lipgloss.NoColor{}
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
	default:
		return lipgloss.NewStyle().Foreground(AIMsgFg)
	}
}
