package anthropic

import (
	"errors"

	"github.com/anthropics/anthropic-sdk-go"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
)

func toProviderErr(err error) error {
	if err == nil {
		return nil
	}
	var apiErr *anthropic.Error
	if errors.As(err, &apiErr) {
		fields := provider.UpstreamErrorFields{Type: string(apiErr.Type())}
		if raw := apiErr.RawJSON(); raw != "" {
			if parsed, ok := provider.ParseUpstreamErrorBody([]byte(raw)); ok {
				fields = parsed
			}
		}
		message := provider.FormatProviderErrorMessage(fields)
		if message == "" {
			message = apiErr.Error()
		}
		out := &provider.ProviderError{
			Title:      provider.ErrorTitleForStatus(apiErr.StatusCode),
			Message:    message,
			ErrorType:  fields.Type,
			ErrorCode:  fields.Code,
			Cause:      apiErr,
			StatusCode: apiErr.StatusCode,
		}
		if apiErr.Response != nil {
			out.ResponseHeaders = provider.HeaderMap(apiErr.Response.Header)
		}
		if apiErr.Request != nil {
			out.URL = apiErr.Request.URL.String()
		}
		out.RequestBody = apiErr.DumpRequest(true)
		out.ResponseBody = apiErr.DumpResponse(true)
		provider.EnrichProviderError(out)
		return out
	}
	return provider.WrapUnexpectedEOF(err)
}
