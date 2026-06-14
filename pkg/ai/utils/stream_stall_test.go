package utils

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestWithStreamStallWatchCancelsOnInactivity(t *testing.T) {
	t.Parallel()

	ctx, bump := WithStreamStallWatch(context.Background(), 30*time.Millisecond)
	bump()
	time.Sleep(80 * time.Millisecond)
	require.Error(t, ctx.Err())
	require.True(t, errors.Is(context.Cause(ctx), ErrStreamStall))
}

func TestWithStreamStallWatchStaysAliveWithActivity(t *testing.T) {
	t.Parallel()

	ctx, bump := WithStreamStallWatch(context.Background(), 50*time.Millisecond)
	for range 4 {
		bump()
		time.Sleep(20 * time.Millisecond)
	}
	require.NoError(t, ctx.Err())
}