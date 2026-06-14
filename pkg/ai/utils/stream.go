package utils

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"

	provider "github.com/riipandi/elph/pkg/ai/protocol"
)

// PostSSE sends a JSON POST request and invokes onData for each SSE data payload.
func PostSSE(ctx context.Context, client *http.Client, url string, headers map[string]string, body any, onData func(data []byte) error) error {
	if client == nil {
		client = NewStreamingHTTPClient()
	}

	streamCtx, bump := WithStreamStallWatch(ctx, StreamStallTimeout)

	payload, err := json.Marshal(body)
	if err != nil {
		return fmt.Errorf("encode request: %w", err)
	}

	req, err := http.NewRequestWithContext(streamCtx, http.MethodPost, url, bytes.NewReader(payload))
	if err != nil {
		return fmt.Errorf("build request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "text/event-stream")
	for key, value := range headers {
		req.Header.Set(key, value)
	}

	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		raw, _ := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
		return upstreamHTTPError(resp.StatusCode, raw)
	}

	bump()
	scanner := bufio.NewScanner(resp.Body)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for scanner.Scan() {
		line := scanner.Text()
		if !strings.HasPrefix(line, "data:") {
			continue
		}
		data := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
		if data == "" || data == "[DONE]" {
			if data == "[DONE]" {
				break
			}
			continue
		}
		bump()
		if err := onData([]byte(data)); err != nil {
			return err
		}
	}
	if err := scanner.Err(); err != nil {
		return fmt.Errorf("read stream: %w", err)
	}
	return nil
}

func upstreamHTTPError(statusCode int, body []byte) error {
	return provider.NewUpstreamHTTPError(statusCode, body)
}
