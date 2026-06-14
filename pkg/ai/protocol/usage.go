package protocol

// TurnUsage reports token usage for a completed provider turn.
type TurnUsage struct {
	InputTokens      int
	OutputTokens     int
	CacheReadTokens  int
	CacheWriteTokens int
}
