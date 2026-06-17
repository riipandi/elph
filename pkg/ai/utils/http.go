package utils

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"

	"resty.dev/v3"
)

const (
	defaultHTTPTimeout       = 120 * time.Second
	streamResponseHeaderWait = 60 * time.Second

	// DefaultRetryCount is the default number of retries for retriable failures.
	DefaultRetryCount = 2

	// DefaultRetryWaitTime is the initial wait time between retries.
	DefaultRetryWaitTime = 1 * time.Second

	// DefaultRetryMaxWaitTime is the maximum wait time between retries.
	DefaultRetryMaxWaitTime = 10 * time.Second
)

// NewHTTPClient returns a resty client with the default upstream timeout.
func NewHTTPClient() *resty.Client {
	return NewHTTPClientWithTimeout(defaultHTTPTimeout)
}

// NewHTTPClientWithTimeout returns a resty client with the given timeout and
// default retry settings (2 retries on 5xx / network errors).
func NewHTTPClientWithTimeout(timeout time.Duration) *resty.Client {
	return resty.New().
		SetTimeout(timeout).
		SetRetryCount(DefaultRetryCount).
		SetRetryWaitTime(DefaultRetryWaitTime).
		SetRetryMaxWaitTime(DefaultRetryMaxWaitTime).
		AddRetryConditions(
			func(r *resty.Response, err error) bool {
				if err != nil {
					return true
				}
				return r.StatusCode() >= 500
			},
		)
}

// NewStreamingHTTPClient returns a resty client tuned for SSE: bounded wait for
// response headers and no overall request timeout (stall watch handles hangs).
func NewStreamingHTTPClient() *resty.Client {
	baseTransport, ok := http.DefaultTransport.(*http.Transport)
	if !ok {
		// http.DefaultTransport is always *http.Transport in practice.
		transport := &http.Transport{
			ResponseHeaderTimeout: streamResponseHeaderWait,
		}
		return resty.New().SetTransport(transport)
	}
	transport := baseTransport.Clone()
	transport.ResponseHeaderTimeout = streamResponseHeaderWait
	return resty.New().SetTransport(transport)
}

// PostJSON sends a JSON request and decodes a JSON response.
func PostJSON(ctx context.Context, client *resty.Client, url string, headers map[string]string, body any, out any) error {
	if client == nil {
		client = NewHTTPClient()
	}

	r := client.R().SetContext(ctx)
	if body != nil {
		r.SetBody(body)
	}
	for k, v := range headers {
		r.SetHeader(k, v)
	}

	resp, err := r.Post(url)
	if err != nil {
		return err
	}
	if resp.StatusCode() < 200 || resp.StatusCode() >= 300 {
		return fmt.Errorf("upstream %s: %s", resp.Status(), trimBody(resp.Bytes()))
	}
	if out == nil {
		return nil
	}
	if err := json.Unmarshal(resp.Bytes(), out); err != nil {
		return fmt.Errorf("decode response: %w", err)
	}
	return nil
}

// GetJSON sends a GET request and decodes a JSON response.
func GetJSON(ctx context.Context, client *resty.Client, url string, out any) error {
	return GetJSONWithHeaders(ctx, client, url, nil, out)
}

// GetJSONWithHeaders sends a GET request with optional headers and decodes JSON.
func GetJSONWithHeaders(ctx context.Context, client *resty.Client, url string, headers map[string]string, out any) error {
	if client == nil {
		client = NewHTTPClient()
	}

	r := client.R().SetContext(ctx)
	for k, v := range headers {
		r.SetHeader(k, v)
	}

	resp, err := r.Get(url)
	if err != nil {
		return err
	}
	if resp.StatusCode() < 200 || resp.StatusCode() >= 300 {
		return fmt.Errorf("upstream %s: %s", resp.Status(), trimBody(resp.Bytes()))
	}
	if out == nil {
		return nil
	}
	if err := json.Unmarshal(resp.Bytes(), out); err != nil {
		return fmt.Errorf("decode response: %w", err)
	}
	return nil
}

func trimBody(raw []byte) string {
	text := strings.TrimSpace(string(raw))
	if len(text) > 240 {
		return text[:240] + "..."
	}
	return text
}
