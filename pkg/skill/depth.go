package skill

import (
	"context"
	"fmt"
)

type depthKey struct{}

// WithDepthHolder attaches a per-turn Skill nesting counter to ctx.
func WithDepthHolder(ctx context.Context) context.Context {
	if holderFrom(ctx) != nil {
		return ctx
	}
	return context.WithValue(ctx, depthKey{}, &depthHolder{})
}

// Enter increments the Skill nesting depth and rejects when MaxNestingDepth is exceeded.
func Enter(ctx context.Context) error {
	holder := holderFrom(ctx)
	if holder == nil {
		return nil
	}
	if holder.n >= MaxNestingDepth {
		return fmt.Errorf("maximum skill nesting depth (%d) exceeded", MaxNestingDepth)
	}
	holder.n++
	return nil
}

type depthHolder struct {
	n int
}

func holderFrom(ctx context.Context) *depthHolder {
	if ctx == nil {
		return nil
	}
	holder, _ := ctx.Value(depthKey{}).(*depthHolder)
	return holder
}
