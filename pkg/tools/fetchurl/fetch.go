// Package fetchurl fetches remote HTTP(S) content for the FetchURL agent tool.
package fetchurl

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"resty.dev/v3"
)

const MaxBytes = 256 << 10

// HTTPClient is used for outbound requests. Tests may replace it.
var HTTPClient = resty.New().
	SetTimeout(30 * time.Second).
	SetRetryCount(2).
	SetRetryWaitTime(1 * time.Second).
	SetRetryMaxWaitTime(5 * time.Second).
	AddRetryConditions(
		func(r *resty.Response, err error) bool {
			if err != nil {
				return true
			}
			return r.StatusCode() >= 500
		},
	).
	SetRedirectPolicy(resty.RedirectPolicyFunc(func(req *http.Request, via []*http.Request) error {
		if len(via) >= 5 {
			return fmt.Errorf("too many redirects")
		}
		if _, err := parsePublicURL(req.URL.String()); err != nil {
			return err
		}
		return nil
	}))

// Result holds normalized fetch output.
type Result struct {
	URL         string
	ContentType string
	Body        string
}

// Fetch downloads a URL and returns sanitized text for HTML or raw text otherwise.
func Fetch(ctx context.Context, rawURL string) (Result, error) {
	u, err := parsePublicURL(rawURL)
	if err != nil {
		return Result{}, err
	}

	resp, err := HTTPClient.R().
		SetContext(ctx).
		SetHeader("User-Agent", "Elph/1.0 (+https://github.com/riipandi/elph)").
		SetHeader("Accept", "text/html,application/json,text/plain,*/*").
		SetResponseDoNotParse(true).
		Get(u.String())
	if err != nil {
		return Result{}, err
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(io.LimitReader(resp.Body, MaxBytes+1))
	if err != nil {
		return Result{}, err
	}
	truncated := len(data) > MaxBytes
	if truncated {
		data = data[:MaxBytes]
	}

	ct := resp.Header().Get("Content-Type")
	body := string(data)
	if isHTMLContentType(ct) {
		body = htmlToText(data)
		if body == "" {
			body = strings.TrimSpace(string(data))
		}
	}

	if !resp.IsStatusSuccess() {
		return Result{}, fmt.Errorf("status %s: %s", resp.Status(), trimBody(body))
	}
	if truncated {
		body += "\n\n(output truncated)"
	}
	return Result{
		URL:         resp.Request.URL,
		ContentType: ct,
		Body:        strings.TrimRight(body, "\n"),
	}, nil
}

// Format renders fetch output for the FetchURL tool.
func Format(r Result) string {
	var b strings.Builder
	fmt.Fprintf(&b, "url: %s\n", r.URL)
	if ct := strings.TrimSpace(r.ContentType); ct != "" {
		fmt.Fprintf(&b, "content_type: %s\n", ct)
	}
	b.WriteString("\n")
	b.WriteString(r.Body)
	return strings.TrimRight(b.String(), "\n")
}

func trimBody(s string) string {
	s = strings.TrimSpace(s)
	if len(s) > 240 {
		return s[:240] + "..."
	}
	return s
}
