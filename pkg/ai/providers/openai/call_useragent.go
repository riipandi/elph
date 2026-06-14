package openai

import (
	provider "github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/riipandi/elph/pkg/ai/providers/internal/httpheaders"
)

func callUserAgent(req provider.TurnRequest) (string, bool) {
	if req.Compat.ThinkingFormat != "" {
		_ = req.Compat.ThinkingFormat
	}
	return httpheaders.CallUserAgent("")
}
