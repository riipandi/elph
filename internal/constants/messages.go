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
	// UserStickyMsgBg is a neutral backdrop for the pinned user prompt header.
	UserStickyMsgBg = compat.AdaptiveColor{Light: lipgloss.Color("#EEEEEE"), Dark: lipgloss.Color("#232323")}
	// UserStickyTimestampFg is a soft green accent for the pinned prompt timestamp.
	UserStickyTimestampFg = compat.AdaptiveColor{Light: lipgloss.Color("#588458"), Dark: lipgloss.Color("#8FB88F")}
	// UserMsgAccent is the left-border accent for user prompt blocks.
	UserMsgAccent = UserStickyTimestampFg

	AIMsgFg = BrightText
	AIMsgBg = lipgloss.NoColor{}

	SystemMsgFg = DimText
	SystemMsgBg = lipgloss.NoColor{}

	ToolMsgFg = compat.AdaptiveColor{Light: lipgloss.Color("#547DA7"), Dark: lipgloss.Color("#67E8F9")}
	ToolMsgBg = compat.AdaptiveColor{Light: lipgloss.Color("#E8E8F0"), Dark: lipgloss.Color("#0C1A1D")}

	ThinkingMsgFg = DimText
	ThinkingMsgBg = compat.AdaptiveColor{Light: lipgloss.Color("#EEEEEE"), Dark: lipgloss.Color("#232323")}
)

// UserLeftBarStyle returns the accent column for user prompt blocks.
func UserLeftBarStyle(bg, accent compat.AdaptiveColor) lipgloss.Style {
	return lipgloss.NewStyle().Foreground(accent).Background(bg)
}

// StickyUserStyle returns the box style for the pinned user prompt header.
func StickyUserStyle() lipgloss.Style {
	return lipgloss.NewStyle().Background(UserStickyMsgBg)
}

// UserMessageBoxStyle returns the collapsible user prompt block style.
func UserMessageBoxStyle() lipgloss.Style {
	return MessageStyle(MessageUser)
}

// StickyUserTitleStyle returns the neutral dim foreground for the sticky prompt preview.
func StickyUserTitleStyle() lipgloss.Style {
	return lipgloss.NewStyle().Foreground(DimText).Background(UserStickyMsgBg)
}

// StickyUserTimestampStyle returns the soft green style for the sticky prompt timestamp.
func StickyUserTimestampStyle() lipgloss.Style {
	return lipgloss.NewStyle().Foreground(UserStickyTimestampFg).Background(UserStickyMsgBg)
}

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
