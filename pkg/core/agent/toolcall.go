package agent

import (
	"regexp"
	"strings"

	"github.com/riipandi/elph/pkg/tools"
)

// ParsedToolCall is a tool invocation embedded in model text output.
type ParsedToolCall struct {
	Name       string
	Parameters map[string]string
}

// StripToolCalls removes embedded toolcall markup and returns parsed invocations.
func StripToolCalls(text string) (string, []ParsedToolCall) {
	if text == "" {
		return "", nil
	}
	ensureToolCallRegex()

	var calls []ParsedToolCall
	if shouldSmartStripFirst(text) {
		cleaned := stripSmartMalformedMarkup(text, &calls)
		if strings.TrimSpace(cleaned) == "" && len(calls) > 0 {
			return "", calls
		}
		text = cleaned
	}

	clean := toolCallBlockRe.ReplaceAllStringFunc(text, func(block string) string {
		inner := toolCallBlockRe.FindStringSubmatch(block)
		if len(inner) < 2 {
			return ""
		}
		if call, ok := parseToolCallInner(inner[1]); ok {
			calls = append(calls, call)
			return ""
		}
		calls = append(calls, extractCallsFromMarkup(inner[1])...)
		return ""
	})
	clean = stripLooseFunctionMarkup(clean, &calls)
	clean = stripBrokenFunctionMarkup(clean, &calls)
	clean = stripOrphanToolCallMarkup(clean, &calls)
	clean = stripFunctionNameFragments(clean, &calls)
	clean = stripLooseParameterMarkup(clean, &calls)
	clean = stripTrailingPartialToolMarkup(clean, &calls)
	clean = stripOrphanClosingTags(clean)
	clean = stripSmartMalformedMarkup(clean, &calls)
	clean = StripExtractedPayloads(clean, calls)
	return strings.TrimSpace(clean), calls
}

func stripOrphanToolCallMarkup(text string, calls *[]ParsedToolCall) string {
	for {
		open := toolCallOpenRe.FindStringIndex(text)
		if open == nil {
			return text
		}
		rest := text[open[1]:]
		close := toolCallCloseRe.FindStringIndex(rest)
		if close == nil {
			*calls = append(*calls, extractCallsFromMarkup(text[open[0]:])...)
			return strings.TrimRight(text[:open[0]], " \t\r\n")
		}
		text = strings.TrimSpace(text[:open[0]] + rest[close[1]:])
	}
}

func stripFunctionNameFragments(text string, calls *[]ParsedToolCall) string {
	return toolFunctionNameFragRe.ReplaceAllStringFunc(text, func(block string) string {
		match := toolFunctionNameFragRe.FindStringSubmatch(block)
		if len(match) < 3 {
			return ""
		}
		if call, ok := parsePartialFunctionBody(match[1], match[2]); ok {
			*calls = append(*calls, call)
		}
		return ""
	})
}

func stripLooseParameterMarkup(text string, calls *[]ParsedToolCall) string {
	_ = calls
	return toolParameterRe.ReplaceAllString(text, "")
}

func stripOrphanClosingTags(text string) string {
	text = toolOrphanCloseRe.ReplaceAllString(text, "")
	lines := strings.Split(text, "\n")
	kept := make([]string, 0, len(lines))
	for _, line := range lines {
		if toolMarkupLineRe.MatchString(line) {
			continue
		}
		kept = append(kept, line)
	}
	return strings.Join(kept, "\n")
}

func stripBrokenFunctionMarkup(text string, calls *[]ParsedToolCall) string {
	return toolFunctionBrokenRe.ReplaceAllStringFunc(text, func(block string) string {
		match := toolFunctionBrokenRe.FindStringSubmatch(block)
		if len(match) < 3 {
			return ""
		}
		if call, ok := parsePartialFunctionBody(match[1], match[2]); ok {
			*calls = append(*calls, call)
		}
		return ""
	})
}

func stripLooseFunctionMarkup(text string, calls *[]ParsedToolCall) string {
	return toolFunctionRe.ReplaceAllStringFunc(text, func(block string) string {
		match := toolFunctionRe.FindStringSubmatch(block)
		if len(match) < 3 {
			return ""
		}
		if call, ok := parseFunctionMatch(match[1], match[2]); ok {
			*calls = append(*calls, call)
		}
		return ""
	})
}

func stripTrailingPartialToolMarkup(text string, calls *[]ParsedToolCall) string {
	for {
		idx := partialToolMarkupTailIndex(text)
		if idx < 0 {
			return text
		}
		*calls = append(*calls, append(extractCallsFromMarkup(text[idx:]), extractSmartToolCalls(text[idx:])...)...)
		text = strings.TrimRight(text[:idx], " \t\r\n")
	}
}

func partialToolMarkupTailIndex(text string) int {
	lower := strings.ToLower(text)
	best := -1
	for _, marker := range []string{"<toolcall", "<tool_call", "<tool-call", "<tool", "<parameter", " search>", "websearch>"} {
		idx := strings.LastIndex(lower, marker)
		if idx < 0 {
			continue
		}
		suffix := lower[idx:]
		if toolCallCloseRe.MatchString(suffix) && toolCallBlockRe.MatchString(text[idx:]) {
			continue
		}
		if !toolCallCloseRe.MatchString(suffix) && idx > best {
			best = idx
		}
	}
	return best
}

func extractCallsFromMarkup(markup string) []ParsedToolCall {
	if call, ok := parseToolCallInner(markup); ok {
		return []ParsedToolCall{call}
	}

	var calls []ParsedToolCall
	for _, match := range toolFunctionRe.FindAllStringSubmatch(markup, -1) {
		if len(match) < 3 {
			continue
		}
		if call, ok := parseFunctionMatch(match[1], match[2]); ok {
			calls = append(calls, call)
		}
	}
	if len(calls) > 0 {
		return calls
	}

	if match := toolFunctionOpenRe.FindStringSubmatch(markup); len(match) >= 3 {
		if call, ok := parsePartialFunctionBody(match[1], match[2]); ok {
			calls = append(calls, call)
		}
	}
	if len(calls) > 0 {
		return calls
	}

	if match := toolFunctionBrokenRe.FindStringSubmatch(markup); len(match) >= 3 {
		if call, ok := parsePartialFunctionBody(match[1], match[2]); ok {
			calls = append(calls, call)
		}
	}
	if len(calls) > 0 {
		return calls
	}

	if match := toolFunctionNameFragRe.FindStringSubmatch(markup); len(match) >= 3 {
		if call, ok := parsePartialFunctionBody(match[1], match[2]); ok {
			calls = append(calls, call)
		}
	}
	return calls
}

// SanitizeParsedToolName trims malformed markup tails from a parsed function name.
func SanitizeParsedToolName(name string) string {
	name = strings.TrimSpace(name)
	if i := strings.IndexAny(name, "</"); i >= 0 {
		name = name[:i]
	}
	return strings.TrimSpace(name)
}

func canonicalParsedToolName(name string) string {
	name = SanitizeParsedToolName(name)
	if name == "" {
		return ""
	}
	if canonical, ok := tools.ResolveName(name); ok {
		return canonical
	}
	return name
}

func parsePartialFunctionBody(name, body string) (ParsedToolCall, bool) {
	name = canonicalParsedToolName(name)
	if name == "" {
		return ParsedToolCall{}, false
	}

	params := parseToolParameters(body)
	if match := toolParameterOpenRe.FindStringSubmatch(body); len(match) >= 3 {
		key := strings.TrimSpace(match[1])
		if key != "" {
			if _, exists := params[key]; !exists {
				params[key] = strings.TrimSpace(match[2])
			}
		}
	}
	if len(params) == 0 {
		return ParsedToolCall{}, false
	}
	return ParsedToolCall{Name: name, Parameters: params}, true
}

func parseToolCallInner(inner string) (ParsedToolCall, bool) {
	match := toolFunctionRe.FindStringSubmatch(strings.TrimSpace(inner))
	if len(match) < 3 {
		return ParsedToolCall{}, false
	}
	return parseFunctionMatch(match[1], match[2])
}

func parseFunctionMatch(name, body string) (ParsedToolCall, bool) {
	name = canonicalParsedToolName(name)
	if name == "" {
		return ParsedToolCall{}, false
	}

	params := parseToolParameters(body)
	return ParsedToolCall{Name: name, Parameters: params}, true
}

func parseToolParameters(body string) map[string]string {
	params := make(map[string]string)
	for _, re := range []*regexp.Regexp{toolParameterRe, toolParameterLooseRe} {
		if re == nil {
			continue
		}
		for _, param := range re.FindAllStringSubmatch(body, -1) {
			if len(param) < 3 {
				continue
			}
			key := strings.TrimSpace(param[1])
			if key == "" {
				continue
			}
			if _, exists := params[key]; !exists {
				params[key] = strings.TrimSpace(param[2])
			}
		}
	}
	return params
}

// ToolCallStreamFilter removes toolcall markup from streamed assistant text.
// Incomplete opening tags are held back until the block completes or Flush runs.
type ToolCallStreamFilter struct {
	holdback string
}

// Process filters a streamed chunk and returns display-safe text plus any
// newly completed tool calls.
func (f *ToolCallStreamFilter) Process(chunk string) (string, []ParsedToolCall) {
	if chunk == "" {
		return "", nil
	}
	ensureToolCallRegex()

	combined := f.holdback + chunk
	f.holdback = ""

	var (
		safe  strings.Builder
		calls []ParsedToolCall
		rest  = combined
	)

	for {
		open := toolCallOpenRe.FindStringIndex(rest)
		if open == nil {
			if isToolMarkupGarbageSegment(rest) {
				f.holdback = rest
				calls = append(calls, extractSmartToolCalls(rest)...)
				break
			}
			f.holdback = trailingToolCallPrefix(rest)
			if cut := len(rest) - len(f.holdback); cut > 0 {
				safe.WriteString(sanitizeToolMarkupSegment(rest[:cut], &calls))
			}
			if f.holdback != "" {
				calls = append(calls, extractCallsFromMarkup(f.holdback)...)
			}
			break
		}

		if open[0] > 0 {
			safe.WriteString(rest[:open[0]])
		}
		rest = rest[open[0]:]

		close := toolCallCloseRe.FindStringIndex(rest)
		if close == nil {
			f.holdback = rest
			calls = append(calls, extractCallsFromMarkup(rest)...)
			break
		}

		end := close[1]
		block := rest[:end]
		rest = rest[end:]

		if stripped, parsed := StripToolCalls(block); len(parsed) > 0 {
			calls = append(calls, parsed...)
		} else if strings.TrimSpace(stripped) != "" {
			safe.WriteString(stripped)
		}
	}

	return strings.TrimSpace(sanitizeToolMarkupSegment(
		stripTrailingPartialToolMarkup(safe.String(), &calls),
		&calls,
	)), calls
}

func sanitizeToolMarkupSegment(text string, calls *[]ParsedToolCall) string {
	if text == "" {
		return ""
	}
	text = stripFunctionNameFragments(text, calls)
	text = stripLooseParameterMarkup(text, calls)
	text = stripOrphanClosingTags(text)
	return stripSmartMalformedMarkup(text, calls)
}

// SanitizeAssistantDisplay removes embedded toolcall markup for UI rendering.
func SanitizeAssistantDisplay(text string) string {
	clean, _ := StripToolCalls(text)
	return clean
}

// Flush parses any held-back suffix at the end of a turn.
func (f *ToolCallStreamFilter) Flush(text string) (string, []ParsedToolCall) {
	if f == nil {
		return StripToolCalls(text)
	}
	holdback := f.holdback
	f.holdback = ""
	if strings.TrimSpace(text) == "" {
		return StripToolCalls(holdback)
	}

	var extra []ParsedToolCall
	combined := text
	if holdback != "" {
		ensureToolCallRegex()
		if toolCallOpenRe.FindStringIndex(text) != nil {
			// TurnDone repeats streamed content; drop stale partial holdback.
			extra = extractCallsFromMarkup(holdback)
			combined = text
		} else {
			combined = holdback + text
		}
	}
	clean, calls := StripToolCalls(combined)
	if len(extra) > 0 {
		calls = append(calls, extra...)
	}
	return clean, calls
}

// Reset clears held-back stream state.
func (f *ToolCallStreamFilter) Reset() {
	f.holdback = ""
}

func trailingToolCallPrefix(s string) string {
	lower := strings.ToLower(s)
	best := -1
	for _, marker := range []string{
		"<toolcall", "<tool_call", "<tool-call", "<tool",
		"<function=", "<function", "<parameter=", "<parameter",
	} {
		if idx := strings.LastIndex(lower, marker); idx > best {
			if idx == len(lower)-len(marker) || strings.HasPrefix(lower[idx:], marker) {
				best = idx
			}
		}
	}
	if tail := partialFunctionTailRe.FindStringIndex(lower); tail != nil {
		if tail[0] > best {
			best = tail[0]
		}
	}
	if best < 0 {
		return ""
	}
	suffix := lower[best:]
	if strings.Contains(suffix, ">") && !strings.Contains(suffix, "</") {
		return s[best:]
	}
	if !strings.Contains(suffix, ">") {
		return s[best:]
	}
	return ""
}
