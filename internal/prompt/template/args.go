package template

import (
	"regexp"
	"strconv"
	"strings"
)

var argPlaceholderRe = regexp.MustCompile(
	`\$\{(\d+):-([^}]*)\}|\$\{@:(\d+)(?::(\d+))?\}|\$(ARGUMENTS|@|\d+)`,
)

// ParseArgs splits a command argument string respecting quoted strings.
func ParseArgs(argsString string) []string {
	args := make([]string, 0)
	current := strings.Builder{}
	inQuote := byte(0)

	for i := 0; i < len(argsString); i++ {
		ch := argsString[i]
		if inQuote != 0 {
			if ch == inQuote {
				inQuote = 0
			} else {
				current.WriteByte(ch)
			}
			continue
		}
		switch ch {
		case '"', '\'':
			inQuote = ch
		case ' ', '\t', '\n', '\r':
			if current.Len() > 0 {
				args = append(args, current.String())
				current.Reset()
			}
		default:
			current.WriteByte(ch)
		}
	}
	if current.Len() > 0 {
		args = append(args, current.String())
	}
	return args
}

// SubstituteArgs replaces $1, $@, ${1:-default}, and ${@:N[:L]} placeholders.
func SubstituteArgs(content string, args []string) string {
	allArgs := strings.Join(args, " ")
	return argPlaceholderRe.ReplaceAllStringFunc(content, func(match string) string {
		sub := argPlaceholderRe.FindStringSubmatch(match)
		if sub == nil {
			return match
		}

		if sub[1] != "" {
			index, _ := strconv.Atoi(sub[1])
			if index > 0 && index <= len(args) && args[index-1] != "" {
				return args[index-1]
			}
			return sub[2]
		}

		if sub[3] != "" {
			start, _ := strconv.Atoi(sub[3])
			if start < 1 {
				start = 1
			}
			startIdx := start - 1
			if sub[4] != "" {
				length, _ := strconv.Atoi(sub[4])
				end := startIdx + length
				if end > len(args) {
					end = len(args)
				}
				if startIdx >= len(args) {
					return ""
				}
				return strings.Join(args[startIdx:end], " ")
			}
			if startIdx >= len(args) {
				return ""
			}
			return strings.Join(args[startIdx:], " ")
		}

		switch sub[5] {
		case "ARGUMENTS", "@":
			return allArgs
		default:
			index, _ := strconv.Atoi(sub[5])
			if index > 0 && index <= len(args) {
				return args[index-1]
			}
			return ""
		}
	})
}
