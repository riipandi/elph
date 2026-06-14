package agent

import (
	"regexp"
	"strings"
	"unicode"

	"github.com/riipandi/elph/pkg/tool"
)

var (
	toolUnnamedParameterRe = regexp.MustCompile(`(?is)<parameter>\s*(.*?)\s*(?:</parameter>|$)`)
	toolParameterAnyRe     = regexp.MustCompile(`(?is)<parameter(?:\s[^>]*)?(?:=([^>\s]+))?>\s*(.*)`)
	embeddedToolNameRe     = regexp.MustCompile(`(?i)(?:^|[^a-z])=([a-z][a-z0-9_]{1,31})>`)
	toolLooseNameCloseRe   = regexp.MustCompile(`(?i)(?:^|[^<a-z])([a-z][a-z0-9_]{2,31})>\s*`)
	toolTagFragmentRe      = regexp.MustCompile(`(?i)</?(?:tool[_-]?call|function|parameter)[^>]*>`)
	toolMangledSplitRe     = regexp.MustCompile(`(?i)=\s*([a-z][a-z0-9_]{1,31})\s*>`)
)

func shouldSmartStripFirst(text string) bool {
	if !containsToolMarkupSignals(text) {
		return false
	}
	if toolCallBlockRe.MatchString(text) || toolFunctionRe.MatchString(text) {
		return false
	}
	return isToolMarkupGarbageSegment(text) || toolUnnamedParameterRe.MatchString(text)
}

func stripSmartMalformedMarkup(text string, calls *[]ParsedToolCall) string {
	if strings.TrimSpace(text) == "" || !containsToolMarkupSignals(text) {
		return text
	}

	garbage := isToolMarkupGarbageSegment(text)

	if inferred := extractSmartToolCalls(text); len(inferred) > 0 {
		*calls = append(*calls, inferred...)
	}

	text = toolTagFragmentRe.ReplaceAllString(text, "")
	text = toolUnnamedParameterRe.ReplaceAllString(text, "")
	text = toolParameterAnyRe.ReplaceAllString(text, "")
	text = embeddedToolNameRe.ReplaceAllString(text, "")
	text = toolLooseNameCloseRe.ReplaceAllString(text, " ")
	text = toolOrphanCloseRe.ReplaceAllString(text, "")
	text = collapseWhitespaceLines(text)
	text = strings.TrimSpace(text)

	if garbage || (text != "" && containsToolMarkupSignals(text)) {
		return ""
	}
	return text
}

func isToolMarkupGarbageSegment(text string) bool {
	if !containsToolMarkupSignals(text) {
		return false
	}
	prefix := prosePrefixBeforeMarkup(text)
	if prefix != "" && !isMarkupArtifact(prefix) {
		return false
	}
	return true
}

func prosePrefixBeforeMarkup(text string) string {
	idx := firstToolMarkupIndex(text)
	if idx < 0 {
		return strings.TrimSpace(text)
	}
	return strings.TrimSpace(text[:idx])
}

func firstToolMarkupIndex(text string) int {
	lower := strings.ToLower(text)
	best := -1
	for _, signal := range []string{
		"<parameter", "<function", "<toolcall", "<tool_call", "<tool-call", "<tool",
	} {
		if idx := strings.Index(lower, signal); idx >= 0 && (best < 0 || idx < best) {
			best = idx
		}
	}
	if m := regexp.MustCompile(`(?i)\s[a-z][a-z0-9_]{2,31}>`).FindStringIndex(text); m != nil && (best < 0 || m[0] < best) {
		best = m[0]
	}
	if m := embeddedToolNameRe.FindStringIndex(text); m != nil && (best < 0 || m[0] < best) {
		best = m[0]
	}
	return best
}

func looksLikePartialTag(text string) bool {
	trimmed := strings.TrimSpace(text)
	return strings.HasPrefix(trimmed, "<") && !strings.Contains(trimmed, ">")
}

func isMarkupArtifact(text string) bool {
	trimmed := strings.TrimSpace(text)
	if trimmed == "" {
		return false
	}
	if containsToolMarkupSignals(trimmed) {
		return true
	}
	return regexp.MustCompile(`(?i)^[a-z][a-z0-9_]{2,31}>\s*$`).MatchString(trimmed)
}

func extractSmartToolCalls(text string) []ParsedToolCall {
	name := inferToolName(text)
	query, paramKey := inferToolPayload(text, name)
	if query != "" && (isMarkupArtifact(query) || looksLikePartialTag(query)) {
		query = ""
	}
	if name == "" && query == "" {
		return nil
	}
	if name == "" {
		name = "ToolRequest"
	}
	if query == "" {
		return []ParsedToolCall{{Name: name, Parameters: map[string]string{}}}
	}
	key := paramKey
	if key == "" {
		key = defaultParamKey(name)
	}
	return []ParsedToolCall{{
		Name:       name,
		Parameters: map[string]string{key: query},
	}}
}

func inferToolName(text string) string {
	lower := strings.ToLower(text)

	for _, match := range embeddedToolNameRe.FindAllStringSubmatch(lower, -1) {
		if canonical := resolveKnownTool(match[1]); canonical != "" {
			return canonical
		}
	}
	for _, match := range toolLooseNameCloseRe.FindAllStringSubmatch(text, -1) {
		if canonical := resolveKnownTool(match[1]); canonical != "" {
			return canonical
		}
	}
	for _, match := range toolMangledSplitRe.FindAllStringSubmatch(lower, -1) {
		if canonical := resolveKnownTool(match[1]); canonical != "" {
			return canonical
		}
	}

	tokens := tokenizeToolHints(lower)
	for _, token := range tokens {
		if canonical := resolveKnownTool(token); canonical != "" {
			return canonical
		}
	}
	return ""
}

func inferToolPayload(text, toolName string) (value, key string) {
	if match := toolParameterAnyRe.FindStringSubmatch(text); len(match) >= 3 {
		key = strings.TrimSpace(match[1])
		body := match[2]
		if end := strings.Index(strings.ToLower(body), "</parameter>"); end >= 0 {
			body = body[:end]
		}
		value = cleanMangledPayload(body, toolName)
		if value != "" {
			return value, key
		}
	}
	if match := toolUnnamedParameterRe.FindStringSubmatch(text); len(match) >= 2 {
		return cleanMangledPayload(match[1], toolName), ""
	}

	remainder := toolTagFragmentRe.ReplaceAllString(text, "")
	remainder = toolLooseNameCloseRe.ReplaceAllString(remainder, " ")
	remainder = embeddedToolNameRe.ReplaceAllString(remainder, " ")
	return cleanMangledPayload(remainder, toolName), ""
}

func cleanMangledPayload(raw, toolName string) string {
	q := strings.TrimSpace(raw)
	if q == "" {
		return ""
	}

	if toolName != "" {
		splitter := regexp.MustCompile(`(?i)=\s*` + regexp.QuoteMeta(strings.ToLower(toolName)) + `\s*>`)
		parts := splitter.Split(q, -1)
		if len(parts) > 1 {
			var merged strings.Builder
			for _, part := range parts {
				part = strings.TrimSpace(part)
				if part == "" {
					continue
				}
				if merged.Len() > 0 {
					merged.WriteByte(' ')
				}
				merged.WriteString(part)
			}
			q = merged.String()
		}
	}

	for _, match := range toolMangledSplitRe.FindAllStringSubmatch(strings.ToLower(q), -1) {
		if canonical := resolveKnownTool(match[1]); canonical != "" && !strings.EqualFold(canonical, toolName) {
			continue
		}
		q = toolMangledSplitRe.ReplaceAllString(q, " ")
	}

	q = toolTagFragmentRe.ReplaceAllString(q, "")
	q = regexp.MustCompile(`(?i)\b[a-z][a-z0-9_]{2,31}>\s*`).ReplaceAllString(q, " ")
	q = splitGluedWords(q)
	q = strings.Join(strings.Fields(q), " ")
	q = regexp.MustCompile(`\s+\d$`).ReplaceAllString(q, "")
	return strings.TrimSpace(q)
}

func splitGluedWords(s string) string {
	if s == "" {
		return s
	}
	var b strings.Builder
	runes := []rune(s)
	for i := 0; i < len(runes); i++ {
		if i > 0 && unicode.IsLower(runes[i-1]) && unicode.IsUpper(runes[i]) {
			b.WriteByte(' ')
		}
		if i > 0 && i+1 < len(runes) && unicode.IsDigit(runes[i]) && unicode.IsLetter(runes[i-1]) && unicode.IsLetter(runes[i+1]) {
			b.WriteByte(' ')
		}
		b.WriteRune(runes[i])
	}
	return b.String()
}

func containsToolMarkupSignals(text string) bool {
	lower := strings.ToLower(text)
	for _, signal := range []string{
		"<parameter", "<function", "<toolcall", "<tool_call", "<tool-call",
		"</parameter", "</function", "</toolcall", "</tool_call",
	} {
		if strings.Contains(lower, signal) {
			return true
		}
	}
	if toolTagFragmentRe.MatchString(text) {
		return true
	}
	if embeddedToolNameRe.MatchString(text) {
		return true
	}
	if regexp.MustCompile(`(?i)\s[a-z][a-z0-9_]{2,31}>\s*`).MatchString(text) {
		return strings.Contains(lower, "parameter") || embeddedToolNameRe.MatchString(text)
	}
	return false
}

func resolveKnownTool(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	if name, ok := tool.ResolveName(raw); ok {
		return name
	}
	compact := strings.NewReplacer("_", "", "-", "").Replace(strings.ToLower(raw))
	for _, def := range tool.All() {
		alias := strings.NewReplacer("_", "", "-", "").Replace(strings.ToLower(def.Name))
		if alias == compact {
			return def.Name
		}
	}
	return ""
}

func tokenizeToolHints(lower string) []string {
	var tokens []string
	for _, match := range regexp.MustCompile(`[a-z][a-z0-9_]{2,31}`).FindAllString(lower, -1) {
		tokens = append(tokens, match)
	}
	return tokens
}

func defaultParamKey(toolName string) string {
	switch strings.ToLower(toolName) {
	case "grep", "codesearch":
		return "pattern"
	case "read", "write", "edit", "glob", "readmediafile":
		return "path"
	case "bash":
		return "command"
	case "fetchurl", "websearch":
		return "query"
	default:
		return "input"
	}
}

func collapseWhitespaceLines(text string) string {
	lines := strings.Split(text, "\n")
	kept := make([]string, 0, len(lines))
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		if containsToolMarkupSignals(line) {
			continue
		}
		kept = append(kept, line)
	}
	return strings.Join(kept, "\n")
}
