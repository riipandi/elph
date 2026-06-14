package renderer

import (
	"time"
)

func messageAtUnix(at time.Time) int64 {
	if at.IsZero() {
		return 0
	}
	return at.Unix()
}

// formatMessageTimestamp renders a compact local timestamp for message blocks.
func formatMessageTimestamp(at time.Time) string {
	if at.IsZero() {
		return ""
	}
	local := at.Local()
	if time.Now().Local().YearDay() == local.YearDay() && time.Now().Local().Year() == local.Year() {
		return local.Format("15:04:05")
	}
	return local.Format("Jan 2 15:04:05")
}
