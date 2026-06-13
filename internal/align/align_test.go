package align

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestRowAlignsSummaryColumn(t *testing.T) {
	name, gap, summary := Row("/help", 12, "Show commands")
	require.Equal(t, "/help", name)
	require.Equal(t, "Show commands", summary)
	require.GreaterOrEqual(t, len(gap), ColumnGap)
}

func TestColumnWidthUsesWidestValue(t *testing.T) {
	require.Greater(t, ColumnWidth("/help", "/diagnostic:open-log"), ColumnWidth("/help"))
}
