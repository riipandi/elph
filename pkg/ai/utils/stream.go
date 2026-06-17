package utils

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	provider "github.com/riipandi/elph/pkg/ai/protocol"
	"resty.dev/v3"
)

// PostSSE sends a JSON POST request and invokes onData for each SSE data payload.
func PostSSE(ctx context.Context, client *resty.Client, url string, headers map[string]string, body any, stallTimeout time.Duration, onData func(data []byte) error) error {
	if client == nil {
		client = NewStreamingHTTPClient()
	}

	streamCtx, bump := WithStreamStallWatch(ctx, EffectiveStreamStallTimeout(stallTimeout))

	payload, err := json.Marshal(body)
	if err != nil {
		return fmt.Errorf("encode request: %w", err)
	}

	var (
		callbackErr error
		httpErr     error
	)

	sse := resty.NewSSESource()
	sse.SetURL(url).
		SetMethod("POST").
		SetBody(bytes.NewReader(payload)).
		SetContext(streamCtx).
		SetTransport(client.Client().Transport).
		SetHeader("Content-Type", "application/json").
		SetHeader("Accept", "text/event-stream").
		OnRequestFailure(func(err error, res *http.Response) {
			if res != nil {
				defer res.Body.Close()
				raw, _ := io.ReadAll(io.LimitReader(res.Body, 1<<20))
				httpErr = upstreamHTTPError(res.StatusCode, raw)
			} else if err != nil {
				httpErr = err
			}
		}).
		OnMessage(func(e any) {
			if callbackErr != nil {
				return
			}
			bump()
			event, ok := e.(*resty.SSE)
			if !ok {
				return
			}
			data := strings.TrimSpace(event.Data)
			if data == "" || data == "[DONE]" {
				if data == "[DONE]" {
					sse.Close()
				}
				return
			}
			if err := onData([]byte(data)); err != nil {
				callbackErr = err
				sse.Close()
			}
		}, nil).
		OnError(func(err error) {
			if callbackErr != nil {
				return
			}
			if errors.Is(err, context.Canceled) && errors.Is(context.Cause(streamCtx), ErrStreamStall) {
				callbackErr = ErrStreamStall
			} else {
				callbackErr = err
			}
		})

	for k, v := range headers {
		sse.SetHeader(k, v)
	}

	if err := sse.Get(); err != nil {
		if httpErr != nil {
			return httpErr
		}
		return err
	}

	if httpErr != nil {
		return httpErr
	}
	return callbackErr
}

func upstreamHTTPError(statusCode int, body []byte) error {
	return provider.NewUpstreamHTTPError(statusCode, body)
}
