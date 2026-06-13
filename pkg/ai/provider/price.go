package provider

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math"
	"strconv"
)

const priceEpsilon = 1e-9

// FormatPrice formats a per-million-token price for display and provider JSON.
// Whole numbers use two decimal places (4 → "4.00").
// Values with at most two decimal places use two decimal places (1.25 → "1.25").
// Values needing more precision keep their significant decimals (0.356 → "0.356").
func FormatPrice(v float64) string {
	if math.IsNaN(v) || math.IsInf(v, 0) {
		return "0.00"
	}
	if math.Abs(v-math.Trunc(v)) < priceEpsilon {
		return fmt.Sprintf("%.2f", v)
	}
	rounded := math.Round(v*100) / 100
	if math.Abs(v-rounded) < priceEpsilon {
		return fmt.Sprintf("%.2f", v)
	}
	return strconv.FormatFloat(v, 'f', -1, 64)
}

// MarshalJSON writes cost fields with consistent decimal formatting.
func (c Cost) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteByte('{')

	first := true
	write := func(key string, value float64) {
		if !first {
			buf.WriteByte(',')
		}
		first = false
		buf.WriteString(`"`)
		buf.WriteString(key)
		buf.WriteString(`":`)
		buf.WriteString(FormatPrice(value))
	}

	write("input", c.Input)
	write("output", c.Output)
	write("cacheRead", c.CacheRead)
	write("cacheWrite", c.CacheWrite)

	buf.WriteByte('}')
	return buf.Bytes(), nil
}

// UnmarshalJSON accepts standard numeric cost values from provider JSON.
func (c *Cost) UnmarshalJSON(data []byte) error {
	var raw struct {
		Input      float64 `json:"input"`
		Output     float64 `json:"output"`
		CacheRead  float64 `json:"cacheRead"`
		CacheWrite float64 `json:"cacheWrite"`
	}
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	c.Input = raw.Input
	c.Output = raw.Output
	c.CacheRead = raw.CacheRead
	c.CacheWrite = raw.CacheWrite
	return nil
}