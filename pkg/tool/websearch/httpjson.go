package websearch

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
)

func urlQueryEscape(s string) string {
	return url.QueryEscape(s)
}

func doJSON(ctx context.Context, client *http.Client, method, rawURL string, headers map[string]string, body any, out any) error {
	var r io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return err
		}
		r = bytes.NewReader(b)
	}
	req, err := http.NewRequestWithContext(ctx, method, rawURL, r)
	if err != nil {
		return err
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	for k, v := range headers {
		req.Header.Set(k, v)
	}
	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	data, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return err
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("status %s: %s", resp.Status, trimHTTPErrorBody(data))
	}
	if out == nil {
		return nil
	}
	return json.Unmarshal(data, out)
}

func trimHTTPErrorBody(data []byte) string {
	s := string(bytes.TrimSpace(data))
	if len(s) > 240 {
		return s[:240] + "..."
	}
	return s
}
