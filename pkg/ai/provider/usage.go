package provider

// TurnCostUSD estimates session cost from per-million-token pricing.
func (c Cost) TurnCostUSD(u TurnUsage) float64 {
	return float64(u.InputTokens)/1e6*c.Input +
		float64(u.OutputTokens)/1e6*c.Output +
		float64(u.CacheReadTokens)/1e6*c.CacheRead +
		float64(u.CacheWriteTokens)/1e6*c.CacheWrite
}

// SupportsImageInput reports whether the model accepts image input.
func SupportsImageInput(inputs []string) bool {
	for _, in := range inputs {
		if in == "image" {
			return true
		}
	}
	return false
}
