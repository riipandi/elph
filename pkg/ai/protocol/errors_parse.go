package protocol

import (
	"encoding/json"
	"regexp"
	"strconv"
	"strings"
)

const providerErrorSummaryMaxLen = 200

// UpstreamErrorFields holds parsed fields from a provider error JSON body.
type UpstreamErrorFields struct {
	Message string
	Type    string
	Code    string
	Param   string
}

// UpstreamErrorFieldsFromParts builds fields from SDK error properties.
func UpstreamErrorFieldsFromParts(message, typ, code, param string) UpstreamErrorFields {
	return UpstreamErrorFields{
		Message: message,
		Type:    typ,
		Code:    code,
		Param:   param,
	}
}

// ParseUpstreamErrorBody parses common provider JSON error envelopes.
func ParseUpstreamErrorBody(body []byte) (UpstreamErrorFields, bool) {
	return parseUpstreamErrorBody(body)
}

// FormatProviderErrorMessage returns a cleaned user-facing provider message.
func FormatProviderErrorMessage(fields UpstreamErrorFields) string {
	return formatProviderErrorMessage(fields)
}

// EnrichProviderError normalizes messages and attaches hints.
func EnrichProviderError(pe *ProviderError) {
	enrichProviderError(pe)
}

var gatewayErrorPrefix = regexp.MustCompile(`(?i)^Error from provider \([^)]+\):\s*`)

// ProviderErrorFromStreamData reports whether an SSE payload is an upstream error event.
func ProviderErrorFromStreamData(data []byte) (*ProviderError, bool) {
	trimmed := strings.TrimSpace(string(data))
	if trimmed == "" {
		return nil, false
	}
	var envelope struct {
		Error   json.RawMessage `json:"error"`
		Type    string          `json:"type"`
		Message string          `json:"message"`
	}
	if err := json.Unmarshal(data, &envelope); err != nil {
		return nil, false
	}
	if len(envelope.Error) == 0 && envelope.Message == "" {
		return nil, false
	}
	if len(envelope.Error) > 0 && string(envelope.Error) != "null" {
		fields, ok := parseUpstreamErrorBody(envelope.Error)
		if !ok {
			return nil, false
		}
		pe := &ProviderError{
			Title:     ErrorTitleForStatus(0),
			Message:   formatProviderErrorMessage(fields),
			ErrorType: fields.Type,
			ErrorCode: fields.Code,
		}
		enrichProviderError(pe)
		return pe, true
	}
	pe := &ProviderError{
		Title:     "provider error",
		Message:   cleanGatewayErrorMessage(envelope.Message),
		ErrorType: envelope.Type,
	}
	enrichProviderError(pe)
	return pe, true
}

func parseUpstreamErrorBody(body []byte) (UpstreamErrorFields, bool) {
	trimmed := strings.TrimSpace(string(body))
	if trimmed == "" {
		return UpstreamErrorFields{}, false
	}

	var envelope struct {
		Type  string `json:"type"`
		Error struct {
			Type    string      `json:"type"`
			Message string      `json:"message"`
			Code    interface{} `json:"code"`
			Param   string      `json:"param"`
		} `json:"error"`
		Message string      `json:"message"`
		Code    interface{} `json:"code"`
		Param   string      `json:"param"`
	}
	if err := json.Unmarshal(body, &envelope); err != nil {
		return UpstreamErrorFields{}, false
	}

	fields := UpstreamErrorFields{}
	switch {
	case envelope.Error.Message != "" || envelope.Error.Type != "":
		fields.Message = envelope.Error.Message
		fields.Type = envelope.Error.Type
		fields.Code = stringifyErrorCode(envelope.Error.Code)
		fields.Param = envelope.Error.Param
	case envelope.Message != "" || envelope.Type != "":
		fields.Message = envelope.Message
		fields.Type = envelope.Type
		fields.Code = stringifyErrorCode(envelope.Code)
		fields.Param = envelope.Param
	default:
		return UpstreamErrorFields{}, false
	}
	return fields, true
}

func formatProviderErrorMessage(fields UpstreamErrorFields) string {
	msg := cleanGatewayErrorMessage(fields.Message)
	if msg == "" {
		if fields.Type != "" {
			return fields.Type
		}
		return ""
	}
	if fields.Type != "" && !strings.HasPrefix(strings.ToLower(msg), strings.ToLower(fields.Type)) {
		return fields.Type + ": " + msg
	}
	return msg
}

func cleanGatewayErrorMessage(message string) string {
	message = strings.TrimSpace(message)
	if message == "" {
		return ""
	}
	return gatewayErrorPrefix.ReplaceAllString(message, "")
}

func stringifyErrorCode(code interface{}) string {
	switch v := code.(type) {
	case string:
		return v
	case float64:
		return strconv.FormatInt(int64(v), 10)
	case json.Number:
		return v.String()
	default:
		return ""
	}
}

func enrichProviderError(pe *ProviderError) {
	if pe == nil {
		return
	}
	applyStatusFromErrorCode(pe)
	pe.Message = cleanGatewayErrorMessage(pe.Message)
	if pe.Message == "" && pe.Title != "" && pe.Title != "provider request failed" {
		pe.Message = pe.Title
	}
	ParseContextTooLarge(pe.Message, pe)
	if pe.Hint == "" {
		pe.Hint = providerErrorHint(pe)
	}
}

func applyStatusFromErrorCode(pe *ProviderError) {
	if pe == nil || pe.StatusCode != 0 || pe.ErrorCode == "" {
		return
	}
	code, err := strconv.Atoi(strings.TrimSpace(pe.ErrorCode))
	if err != nil || code < 400 || code > 599 {
		return
	}
	pe.StatusCode = code
	pe.Title = ErrorTitleForStatus(code)
}

func providerErrorHint(pe *ProviderError) string {
	if pe == nil {
		return ""
	}
	msg := strings.ToLower(pe.Message)
	switch {
	case pe.StatusCode == 401 || strings.Contains(msg, "invalid api key") || strings.Contains(msg, "incorrect api key"):
		return "Check your API key in ~/.elph/providers/."
	case pe.StatusCode == 403:
		return "Your API key may not have access to this model."
	case pe.StatusCode == 429 || strings.Contains(msg, "rate limit") || strings.Contains(msg, "usage limit"):
		return "Wait a moment and retry, or switch models."
	case strings.Contains(msg, "developer") && (strings.Contains(msg, "unknown variant") || strings.Contains(msg, "not supported")):
		return "This model does not support the developer role. Disable thinking or choose another model."
	case pe.IsContextTooLarge():
		return "Shorten the conversation or start a new session."
	case strings.Contains(msg, "idle timeout") || pe.StatusCode == 504:
		return "The gateway timed out waiting for the model (common with slow thinking). Elph will retry without streaming when possible — otherwise retry, disable thinking, or switch models."
	case pe.StatusCode >= 500 || strings.Contains(msg, "overloaded") || strings.Contains(msg, "unavailable"):
		return "The provider may be overloaded — try again or switch models."
	case strings.Contains(msg, "model") && (strings.Contains(msg, "not found") || strings.Contains(msg, "does not exist")):
		return "Check the model id in your provider config."
	}
	return ""
}

func truncateProviderSummary(message string, max int) string {
	message = strings.TrimSpace(message)
	if message == "" || len(message) <= max {
		return message
	}
	return message[:max] + "..."
}
