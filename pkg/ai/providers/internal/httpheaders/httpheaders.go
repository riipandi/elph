// Package httpheaders provides shared User-Agent resolution for HTTP-based providers.
package httpheaders

import (
	"fmt"
	"strings"
)

const defaultProduct = "Elph"

// DefaultUserAgent returns the default User-Agent string for provider SDK calls.
func DefaultUserAgent(version string) string {
	if version == "" {
		version = "dev"
	}
	return fmt.Sprintf("%s/%s (https://github.com/riipandi/elph)", defaultProduct, version)
}

// ResolveHeaders returns a copy of headers with a resolved User-Agent field.
func ResolveHeaders(headers map[string]string, explicitUA, defaultUA string) map[string]string {
	out := make(map[string]string, len(headers)+1)
	var uaKeys []string

	for k, v := range headers {
		out[k] = v
		if strings.EqualFold(k, "User-Agent") {
			uaKeys = append(uaKeys, k)
		}
	}

	switch {
	case explicitUA != "":
		for _, k := range uaKeys {
			delete(out, k)
		}
		out["User-Agent"] = explicitUA
	case len(uaKeys) > 0:
		val := out[uaKeys[0]]
		for _, k := range uaKeys {
			delete(out, k)
		}
		out["User-Agent"] = val
	default:
		out["User-Agent"] = defaultUA
	}
	return out
}

// CallUserAgent resolves a per-call User-Agent override.
func CallUserAgent(callUA string) (string, bool) {
	if callUA != "" {
		return callUA, true
	}
	return "", false
}
