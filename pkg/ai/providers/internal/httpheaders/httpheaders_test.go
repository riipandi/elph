package httpheaders

import "testing"

func TestResolveHeadersExplicitUA(t *testing.T) {
	got := ResolveHeaders(map[string]string{"User-Agent": "old"}, "new", DefaultUserAgent("1.0"))
	if got["User-Agent"] != "new" {
		t.Fatalf("got %q", got["User-Agent"])
	}
}
