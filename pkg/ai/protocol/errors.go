package protocol

import (
	"errors"
	"io"
	"net/http"
	"regexp"
	"strconv"
)

// ProviderError is a normalized upstream provider failure.
type ProviderError struct {
	Title             string
	Message           string
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
	default:
		if code >= 500 {
			return "upstream error"
		}
		return "provider request failed"
	}
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
