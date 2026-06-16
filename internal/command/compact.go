package command

func compactHandler(ctx *Context, args string) string {
	ctx.CompactHistory = true
	ctx.CompactRatio = 0
	ctx.pendingDetailLabel = "Compact history"
	ctx.pendingDetailBody = "Compaction requested."
	return ""
}
