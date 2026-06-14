package anthropic

import "testing"

func TestSanitizeModelID(t *testing.T) {
	if got := SanitizeModelID("  claude-sonnet  "); got != "claude-sonnet" {
		t.Fatalf("got %q", got)
	}
}
