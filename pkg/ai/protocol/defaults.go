package protocol

const defaultMaxTokens = 16384

// DefaultMaxTokens is the fallback completion token limit for providers.
const DefaultMaxTokens = defaultMaxTokens

// MaxTokensOrDefault returns n when positive, otherwise DefaultMaxTokens.
func MaxTokensOrDefault(n int) int {
	if n > 0 {
		return n
	}
	return DefaultMaxTokens
}
