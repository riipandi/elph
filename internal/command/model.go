package command

import (
	"strings"

	"github.com/riipandi/elph/pkg/ai/provider"
)

func modelHandler(ctx *Context, args string) string {
	if _, _, err := provider.EnsureStarterProviders(); err != nil {
		return "/model: " + err.Error()
	}
	catalog := ctx.Catalog
	if len(catalog.Providers) == 0 {
		catalog, _ = provider.LoadCatalog("")
	}
	if len(catalog.Providers) == 0 {
		return "/model: no providers configured — run: elph provider connect"
	}

	ctx.pendingOpenSelector = true
	ctx.selectorCatalog = catalog
	ctx.selectorQuery = strings.TrimSpace(args)
	return ""
}
