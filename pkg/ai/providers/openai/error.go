package openai

import (
	"errors"
	"io"

	openaisdk "github.com/openai/openai-go/v3"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
)

func toProviderErr(err error) error {
	if err == nil {
		return nil
	}
	var apiErr *openaisdk.Error
	if errors.As(err, &apiErr) {
		message := apiErr.Message
		if message == "" && apiErr.Response != nil && apiErr.Response.Body != nil {
			data, _ := io.ReadAll(apiErr.Response.Body)
			message = string(data)
		}
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
			out.RequestBody = apiErr.DumpRequest(true)
		}
		if apiErr.Response != nil {
			out.ResponseBody = apiErr.DumpResponse(true)
		}
		provider.ParseContextTooLarge(message, out)
		return out
	}
	return provider.WrapUnexpectedEOF(err)
}
