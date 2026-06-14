package anthropic

import "github.com/riipandi/elph/pkg/ai/providers/internal/httpheaders"

func defaultUserAgent() string {
	return httpheaders.DefaultUserAgent("")
}
