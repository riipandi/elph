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
		fields := provider.UpstreamErrorFieldsFromParts(
			apiErr.Message,
			apiErr.Type,
			apiErr.Code,
			apiErr.Param,
		)
		if fields.Message == "" && fields.Type == "" {
			if raw := apiErr.RawJSON(); raw != "" {
				if parsed, ok := provider.ParseUpstreamErrorBody([]byte(raw)); ok {
					fields = parsed
				}
			}
		}
		if fields.Message == "" && apiErr.Response != nil && apiErr.Response.Body != nil {
			data, _ := io.ReadAll(apiErr.Response.Body)
			if parsed, ok := provider.ParseUpstreamErrorBody(data); ok {
				fields = parsed
			} else if len(data) > 0 {
				fields.Message = string(data)
			}
		}
		out := &provider.ProviderError{
			Title:      provider.ErrorTitleForStatus(apiErr.StatusCode),
			Message:    provider.FormatProviderErrorMessage(fields),
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
			out.RequestBody = apiErr.DumpRequest(true)
		}
		if apiErr.Response != nil {
			out.ResponseBody = apiErr.DumpResponse(true)
		}
		provider.EnrichProviderError(out)
		return out
	}
	return provider.WrapUnexpectedEOF(err)
}
