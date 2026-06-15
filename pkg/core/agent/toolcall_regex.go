package agent

import (
	"regexp"
	"sync"
)

var (
	toolCallRegexOnce      sync.Once
	toolCallBlockRe        *regexp.Regexp
	toolFunctionRe         *regexp.Regexp
	toolParameterRe        *regexp.Regexp
	toolCallOpenRe         *regexp.Regexp
	toolCallCloseRe        *regexp.Regexp
	toolFunctionOpenRe     *regexp.Regexp
	toolParameterOpenRe    *regexp.Regexp
	toolParameterLooseRe   *regexp.Regexp
	toolFunctionBrokenRe   *regexp.Regexp
	toolFunctionNameFragRe *regexp.Regexp
	toolOrphanCloseRe      *regexp.Regexp
	toolMarkupLineRe       *regexp.Regexp
	partialFunctionTailRe  *regexp.Regexp
)

func ensureToolCallRegex() {
	toolCallRegexOnce.Do(func() {
		toolCallBlockRe = regexp.MustCompile(`(?is)<tool[_-]?call\s*>\s*(.*?)\s*</tool[_-]?call\s*>`)
		toolFunctionRe = regexp.MustCompile(`(?is)<function=([^<>\s/]+)>\s*(.*?)\s*</function>`)
		toolParameterRe = regexp.MustCompile(`(?is)<parameter=([^<>\s/]+)>\s*(.*?)\s*</parameter>`)
		toolCallOpenRe = regexp.MustCompile(`(?i)<tool[_-]?call\s*>`)
		toolCallCloseRe = regexp.MustCompile(`(?i)</tool[_-]?call\s*>`)
		toolFunctionOpenRe = regexp.MustCompile(`(?is)<function=([^<>\s/]+)>\s*(.*)$`)
		toolParameterOpenRe = regexp.MustCompile(`(?is)<parameter=([^<>\s/]+)>\s*(.*)$`)
		toolParameterLooseRe = regexp.MustCompile(`(?is)</?parameter=([^<>\s/]+)>\s*(.*?)\s*</parameter>`)
		toolFunctionBrokenRe = regexp.MustCompile(`(?is)<function=([^<>\s/]+)\s*(.*?)(?:</function>|$)`)
		toolFunctionNameFragRe = regexp.MustCompile(`(?is)=([A-Za-z][\w.-]*)>\s*((?:<parameter=[^>]+>.*?</parameter>\s*)+)`)
		toolOrphanCloseRe = regexp.MustCompile(`(?i)</(?:tool[_-]?call|function|parameter)\s*>`)
		toolMarkupLineRe = regexp.MustCompile(`(?i)^\s*</?(?:tool[_-]?call|function|parameter)(?:\s[^>]*)?>\s*$`)
		partialFunctionTailRe = regexp.MustCompile(`(?i)(?:<function=?|=)[A-Za-z][\w.-]*$`)
	})
}
