package agent

import (
	"strings"
)

type thinkTagPair struct {
	open  string
	close string
}

// Order matters: <redacted_thinking> is what the system prompt asks models to use.
var thinkTagPairs = []thinkTagPair{
	{open: "<redacted_thinking>", close: "</redacted_thinking>"},
	{open: "<think>", close: "</think>"},
	{open: "` <think>", close: "` </think>"},
}

// ThinkTagStreamFilter extracts thinking blocks from streamed assistant text.
// Incomplete tags are held back across chunks.
type ThinkTagStreamFilter struct {
	holdback string
	inThink  bool
	closeTag string
}

// Reset clears held-back stream state.
func (f *ThinkTagStreamFilter) Reset() {
	f.holdback = ""
	f.inThink = false
	f.closeTag = ""
}

// Process splits a streamed chunk into display-safe response text and thinking text.
func (f *ThinkTagStreamFilter) Process(chunk string) (response string, thinking string) {
	if chunk == "" && f.holdback == "" {
		return "", ""
	}

	var (
		respB  strings.Builder
		thinkB strings.Builder
	)
	rest := f.holdback + chunk
	f.holdback = ""

	for rest != "" {
		if f.inThink {
			closeAt := strings.Index(rest, f.closeTag)
			if closeAt < 0 {
				prefix, hold := splitTrailingTagClose(rest, f.closeTag)
				if prefix != "" {
					thinkB.WriteString(prefix)
				}
				f.holdback = hold
				break
			}
			thinkB.WriteString(rest[:closeAt])
			rest = rest[closeAt+len(f.closeTag):]
			f.inThink = false
			f.closeTag = ""
			continue
		}

		openAt, openLen, closeTag, ok := findEarliestThinkOpen(rest)
		if !ok {
			prefix, hold := splitTrailingThinkOpen(rest)
			if prefix != "" {
				respB.WriteString(prefix)
			}
			f.holdback = hold
			break
		}
		if openAt > 0 {
			respB.WriteString(rest[:openAt])
		}
		rest = rest[openAt+openLen:]
		f.inThink = true
		f.closeTag = closeTag
	}

	return respB.String(), thinkB.String()
}

// Flush parses any held-back suffix at the end of a turn.
func (f *ThinkTagStreamFilter) Flush(text string) (response string, thinking string) {
	if text != "" {
		resp, think := f.Process(text)
		if f.holdback != "" {
			if f.inThink {
				think += f.holdback
			} else {
				resp += f.holdback
			}
		}
		f.Reset()
		return resp, think
	}
	if f.inThink {
		thinking = f.holdback
	} else {
		response = f.holdback
	}
	f.Reset()
	return response, thinking
}

// ExtractThinkTags removes all complete thinking blocks from text and returns
// the joined thinking body plus the remaining response text.
func ExtractThinkTags(text string) (thinking string, response string) {
	if text == "" {
		return "", ""
	}

	var (
		thinkB strings.Builder
		respB  strings.Builder
		rest   = text
	)

	for rest != "" {
		openAt, openLen, closeTag, ok := findEarliestThinkOpen(rest)
		if !ok {
			respB.WriteString(rest)
			break
		}
		if openAt > 0 {
			respB.WriteString(rest[:openAt])
		}
		rest = rest[openAt+openLen:]

		closeAt := strings.Index(rest, closeTag)
		if closeAt < 0 {
			thinkB.WriteString(rest)
			break
		}
		thinkB.WriteString(rest[:closeAt])
		rest = rest[closeAt+len(closeTag):]
	}

	return strings.TrimSpace(thinkB.String()), strings.TrimSpace(respB.String())
}

func findEarliestThinkOpen(s string) (index, openLen int, closeTag string, ok bool) {
	best := -1
	for _, pair := range thinkTagPairs {
		if at := strings.Index(s, pair.open); at >= 0 && (best < 0 || at < best) {
			best = at
			openLen = len(pair.open)
			closeTag = pair.close
		}
	}
	return best, openLen, closeTag, best >= 0
}

func splitTrailingTagClose(s, closeTag string) (prefix, holdback string) {
	lower := strings.ToLower(s)
	tag := strings.ToLower(closeTag)
	if idx := strings.LastIndex(lower, "<"); idx >= 0 {
		tail := lower[idx:]
		if len(tail) < len(tag) && strings.HasPrefix(tag, tail) {
			return s[:idx], s[idx:]
		}
	}
	return s, ""
}

func splitTrailingThinkOpen(s string) (prefix, holdback string) {
	lower := strings.ToLower(s)
	best := -1
	for _, pair := range thinkTagPairs {
		open := strings.ToLower(pair.open)
		for i := 1; i <= len(open) && i <= len(lower); i++ {
			suffix := lower[len(lower)-i:]
			if strings.HasPrefix(open, suffix) {
				at := len(s) - i
				if best < 0 || at < best {
					best = at
				}
			}
		}
		if idx := strings.LastIndex(lower, "<"); idx >= 0 {
			tail := lower[idx:]
			if len(tail) < len(open) && strings.HasPrefix(open, tail) && (best < 0 || idx < best) {
				best = idx
			}
		}
	}
	if best < 0 {
		return s, ""
	}
	return s[:best], s[best:]
}
