package utils

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

const (
	defaultHTTPTimeout       = 120 * time.Second
	streamResponseHeaderWait = 60 * time.Second
)

// NewHTTPClient returns a client with the default upstream timeout.
func NewHTTPClient() *http.Client {
	return &http.Client{Timeout: defaultHTTPTimeout}
}

// NewStreamingHTTPClient returns a client tuned for SSE: bounded wait for
// response headers and no overall request timeout (stall watch handles hangs).
func NewStreamingHTTPClient() *http.Client {
	baseTransport, ok := http.DefaultTransport.(*http.Transport)
	if !ok {
		// http.DefaultTransport is always *http.Transport in practice.
		return &http.Client{
			Timeout: 0,
			Transport: &http.Transport{
				ResponseHeaderTimeout: streamResponseHeaderWait,
			},
		}
	}
	transport := baseTransport.Clone()
	transport.ResponseHeaderTimeout = streamResponseHeaderWait
	return &http.Client{
		Timeout:   0,
		Transport: transport,
	}
}

// PostJSON sends a JSON request and decodes a JSON response.
func PostJSON(ctx context.Context, client *http.Client, url string, headers map[string]string, body any, out any) error {
	payload, err := json.Marshal(body)
	if err != nil {
		return fmt.Errorf("encode request: %w", err)
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(payload))
	if err != nil {
		return fmt.Errorf("build request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	for key, value := range headers {
		req.Header.Set(key, value)
	}

	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return fmt.Errorf("read response: %w", err)
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("upstream %s: %s", resp.Status, trimBody(raw))
	}
	if out == nil {
		return nil
	}
	if err := json.Unmarshal(raw, out); err != nil {
		return fmt.Errorf("decode response: %w", err)
	}
	return nil
}

// GetJSON sends a GET request and decodes a JSON response.
func GetJSON(ctx context.Context, client *http.Client, url string, out any) error {
	return GetJSONWithHeaders(ctx, client, url, nil, out)
}

// GetJSONWithHeaders sends a GET request with optional headers and decodes JSON.
func GetJSONWithHeaders(ctx context.Context, client *http.Client, url string, headers map[string]string, out any) error {
	if client == nil {
		client = NewHTTPClient()
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return fmt.Errorf("build request: %w", err)
	}
	for key, value := range headers {
		req.Header.Set(key, value)
	}

	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(io.LimitReader(resp.Body, 32<<20))
	if err != nil {
		return fmt.Errorf("read response: %w", err)
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("upstream %s: %s", resp.Status, trimBody(raw))
	}
	if out == nil {
		return nil
	}
	if err := json.Unmarshal(raw, out); err != nil {
		return fmt.Errorf("decode response: %w", err)
	}
	return nil
}

func trimBody(raw []byte) string {
	text := string(bytes.TrimSpace(raw))
	if len(text) > 240 {
		return text[:240] + "..."
	}
	return text
}
