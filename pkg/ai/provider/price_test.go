package provider

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFormatPrice(t *testing.T) {
	tests := []struct {
		value float64
		want  string
	}{
		{4, "4.00"},
		{10, "10.00"},
		{0, "0.00"},
		{1.25, "1.25"},
		{2.5, "2.50"},
		{0.6, "0.60"},
		{0.15, "0.15"},
		{1.1, "1.10"},
		{3.75, "3.75"},
		{18.75, "18.75"},
		{0.356, "0.356"},
		{0.075, "0.075"},
		{0.028, "0.028"},
	}

	for _, tc := range tests {
		require.Equal(t, tc.want, FormatPrice(tc.value), "value=%v", tc.value)
	}
}

func TestCostMarshalJSONFormatsPrices(t *testing.T) {
	raw, err := json.Marshal(Cost{
		Input:      4,
		Output:     1.25,
		CacheRead:  0.356,
		CacheWrite: 0,
	})
	require.NoError(t, err)
	require.JSONEq(t, `{
		"input": 4.00,
		"output": 1.25,
		"cacheRead": 0.356,
		"cacheWrite": 0.00
	}`, string(raw))
}

func TestCostUnmarshalJSON(t *testing.T) {
	var cost Cost
	require.NoError(t, json.Unmarshal([]byte(`{
		"input": 3,
		"output": 15.5,
		"cacheRead": 0.356
	}`), &cost))
	require.Equal(t, 3.0, cost.Input)
	require.Equal(t, 15.5, cost.Output)
	require.Equal(t, 0.356, cost.CacheRead)
	require.Equal(t, 0.0, cost.CacheWrite)
}