package constants

// ─── Thinking Level ──────────────────────────────────────────────────────────

type ThinkingLevel string

const (
	ThinkingOff     ThinkingLevel = "off"
	ThinkingMinimal ThinkingLevel = "minimal"
	ThinkingLow     ThinkingLevel = "low"
	ThinkingMedium  ThinkingLevel = "medium"
	ThinkingHigh    ThinkingLevel = "high"
	ThinkingXHigh   ThinkingLevel = "xhigh"
)

var thinkingLevels = []ThinkingLevel{
	ThinkingOff,
	ThinkingMinimal,
	ThinkingLow,
	ThinkingMedium,
	ThinkingHigh,
	ThinkingXHigh,
}

func NextThinkingLevel(lvl ThinkingLevel) ThinkingLevel {
	for i, l := range thinkingLevels {
		if l == lvl {
			return thinkingLevels[(i+1)%len(thinkingLevels)]
		}
	}
	return thinkingLevels[0]
}

func PrevThinkingLevel(lvl ThinkingLevel) ThinkingLevel {
	for i, l := range thinkingLevels {
		if l == lvl {
			p := i - 1
			if p < 0 {
				p = len(thinkingLevels) - 1
			}
			return thinkingLevels[p]
		}
	}
	return thinkingLevels[len(thinkingLevels)-1]
}
