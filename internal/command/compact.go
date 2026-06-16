package command

import (
	"fmt"
	"strconv"
	"strings"
)

func compactHandler(ctx *Context, args string) string {
	ratio := 0 // 0 = use default
	if trimmed := strings.TrimSpace(args); trimmed != "" {
		if n, err := strconv.Atoi(trimmed); err == nil && n > 0 && n <= 100 {
			ratio = n
		} else {
			return fmt.Sprintf("/compact: invalid percentage %q — use a number between 1 and 100", trimmed)
		}
	}

	ctx.CompactHistory = true
	ctx.CompactRatio = ratio
	ctx.pendingDetailLabel = "Compact history"
	displayRatio := ratio
	if displayRatio <= 0 {
		displayRatio = 80
	}
	ctx.pendingDetailBody = fmt.Sprintf("Compaction at %d%% requested.", displayRatio)
	return ""
}
