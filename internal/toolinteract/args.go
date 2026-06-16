package toolinteract

import (
	"fmt"
	"strings"
)

// StringArg returns a trimmed string argument when present.
func StringArg(args map[string]any, key string) (string, bool) {
	raw, ok := args[key]
	if !ok || raw == nil {
		return "", false
	}
	switch v := raw.(type) {
	case string:
		s := strings.TrimSpace(v)
		return s, s != ""
	default:
		s := strings.TrimSpace(fmt.Sprint(v))
		return s, s != ""
	}
}

// SortedArgKeys returns argument keys in sorted order.
func SortedArgKeys(args map[string]any) []string {
	keys := make([]string, 0, len(args))
	for k := range args {
		keys = append(keys, k)
	}
	sortStrings(keys)
	return keys
}

func sortStrings(ss []string) {
	for i := 1; i < len(ss); i++ {
		for j := i; j > 0 && ss[j] < ss[j-1]; j-- {
			ss[j], ss[j-1] = ss[j-1], ss[j]
		}
	}
}

// BashCommandArg extracts the bash command string from tool args.
func BashCommandArg(args map[string]any) (string, bool) {
	raw, ok := args["command"]
	if !ok || raw == nil {
		return "", false
	}
	cmd, ok := raw.(string)
	if !ok {
		return "", false
	}
	cmd = strings.TrimSpace(cmd)
	return cmd, cmd != ""
}
