package protocol

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"regexp"
	"strconv"
	"strings"
)

// ProviderError is a normalized upstream provider failure.
type ProviderError struct {
	Title             string
	Message           string
	Hint              string
	ErrorType         string
	ErrorCode         string
	Cause             error
	URL               string
	StatusCode        int
	RequestBody       []byte
	ResponseHeaders   map[string]string
	ResponseBody      []byte
	ContextTooLarge   bool
	ContextMaxTokens  int
	ContextUsedTokens int
}

func (e *ProviderError) Error() string {
	if e == nil {
		return "provider error"
	}
	if e.Message != "" {
		return e.Message
	}
	if e.Title != "" {
		return e.Title
	}
	if e.Cause != nil {
		return e.Cause.Error()
	}
	return "provider error"
}

func (e *ProviderError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Cause
}

// IsRetriable reports whether the error may succeed on retry.
func (e *ProviderError) IsRetriable() bool {
	if e == nil {
		return false
	}
	if errors.Is(e.Cause, io.ErrUnexpectedEOF) {
		return true
	}
	switch e.StatusCode {
	case http.StatusTooManyRequests, http.StatusBadGateway, http.StatusServiceUnavailable, http.StatusGatewayTimeout:
		return true
	}
	return false
}

// IsContextTooLarge reports whether the request exceeded model context limits.
func (e *ProviderError) IsContextTooLarge() bool {
	return e != nil && e.ContextTooLarge
}

var (
	openAIContextPattern  = regexp.MustCompile(`maximum context length (?:is|of) (\d+) tokens.*?(?:resulted in|requested) ~?(\d+) tokens`)
	alibabaContextPattern = regexp.MustCompile(`Range of input length should be \[\d+,\s*(\d+)\]`)
)

// ParseContextTooLarge annotates out when message indicates a context window overflow.
func ParseContextTooLarge(message string, out *ProviderError) {
	if matches := openAIContextPattern.FindStringSubmatch(message); matches != nil {
		out.ContextTooLarge = true
		out.ContextMaxTokens, _ = strconv.Atoi(matches[1])
		out.ContextUsedTokens, _ = strconv.Atoi(matches[2])
		return
	}
	if matches := alibabaContextPattern.FindStringSubmatch(message); matches != nil {
		out.ContextTooLarge = true
		out.ContextMaxTokens, _ = strconv.Atoi(matches[1])
	}
}

// HeaderMap copies the last value for each response header key.
func HeaderMap(in http.Header) map[string]string {
	if len(in) == 0 {
		return nil
	}
	out := make(map[string]string, len(in))
	for k, v := range in {
		if len(v) > 0 {
			out[k] = v[len(v)-1]
		}
	}
	return out
}

// ErrorTitleForStatus returns a short title for an HTTP status code.
func ErrorTitleForStatus(code int) string {
	switch code {
	case http.StatusUnauthorized:
		return "unauthorized"
	case http.StatusForbidden:
		return "forbidden"
	case http.StatusTooManyRequests:
		return "rate limited"
	case http.StatusBadRequest:
		return "bad request"
	case http.StatusGatewayTimeout:
		return "gateway timeout"
	default:
		if code >= 500 {
			return "upstream error"
		}
		return "provider request failed"
	}
}

// ShouldStreamNonStreamingFallback reports whether a failed stream should retry once without streaming.
func ShouldStreamNonStreamingFallback(err error) bool {
	if err == nil {
		return false
	}
	if IsStreamJSONError(err) {
		return true
	}
	var pe *ProviderError
	if !errors.As(err, &pe) || pe == nil {
		return false
	}
	if pe.Title == "stream stalled" || pe.Title == "gateway timeout" {
		return true
	}
	if pe.StatusCode == http.StatusGatewayTimeout {
		return true
	}
	msg := strings.ToLower(pe.Message)
	return strings.Contains(msg, "idle timeout") || strings.Contains(msg, "gateway timeout")
}

// IsStreamJSONError reports whether err is a JSON decode failure from SSE streaming.
func IsStreamJSONError(err error) bool {
	if err == nil {
		return false
	}
	var syntax *json.SyntaxError
	if errors.As(err, &syntax) {
		return true
	}
	msg := err.Error()
	return strings.Contains(msg, "unexpected end of JSON input")
}

// NewUpstreamHTTPError builds a ProviderError from a non-2xx upstream body.
func NewUpstreamHTTPError(statusCode int, body []byte) error {
	pe := &ProviderError{
		Title:        ErrorTitleForStatus(statusCode),
		StatusCode:   statusCode,
		ResponseBody: append([]byte(nil), body...),
	}
	if fields, ok := parseUpstreamErrorBody(body); ok {
		pe.Message = formatProviderErrorMessage(fields)
		pe.ErrorType = fields.Type
		pe.ErrorCode = fields.Code
	}
	if pe.Message == "" {
		pe.Message = trimErrorBody(body)
	}
	enrichProviderError(pe)
	return pe
}

func trimErrorBody(raw []byte) string {
	text := strings.TrimSpace(string(raw))
	if len(text) > 240 {
		return text[:240] + "..."
	}
	return text
}

// WrapStreamError normalizes stall/cancel failures from streaming.
func WrapStreamError(err error) error {
	if err == nil {
		return nil
	}
	msg := err.Error()
	if strings.Contains(msg, "stream stalled") {
		pe := &ProviderError{
			Title:   "stream stalled",
			Message: "No data received from the provider. The model may be overloaded, rate-limited, or unavailable — try again or switch models.",
			Cause:   err,
		}
		enrichProviderError(pe)
		return pe
	}
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return &ProviderError{
			Title:   "stream cancelled",
			Message: err.Error(),
			Cause:   err,
		}
	}
	if wrapped := WrapUnexpectedEOF(err); wrapped != err {
		if pe, ok := wrapped.(*ProviderError); ok {
			enrichProviderError(pe)
		}
		return wrapped
	}
	var pe *ProviderError
	if errors.As(err, &pe) {
		enrichProviderError(pe)
		return pe
	}
	return err
}

// WrapUnexpectedEOF normalizes stream transport failures.
func WrapUnexpectedEOF(err error) error {
	if errors.Is(err, io.ErrUnexpectedEOF) {
		return &ProviderError{
			Title:   "stream transport error",
			Message: err.Error(),
			Cause:   err,
		}
	}
	return err
}
