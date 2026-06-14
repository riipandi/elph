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
		message := apiErr.Error()
		out := &provider.ProviderError{
			Title:      provider.ErrorTitleForStatus(apiErr.StatusCode),
			Message:    message,
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
		provider.ParseContextTooLarge(message, out)
		return out
	}
	return provider.WrapUnexpectedEOF(err)
}
