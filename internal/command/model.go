package command

import (
	"strings"

	"github.com/riipandi/elph/pkg/ai/provider"
)

func modelHandler(ctx *Context, args string) string {
	catalog := ctx.Catalog
	if len(catalog.Providers) == 0 {
		catalog, _ = provider.LoadCatalog("")
	}
	if len(catalog.Providers) == 0 {
		return "/model: no providers found — add JSON files to ~/.elph/providers"
	}

	ctx.pendingOpenSelector = true
	ctx.selectorCatalog = catalog
	ctx.selectorQuery = strings.TrimSpace(args)
	return ""
}
