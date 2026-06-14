package protocol

import (
	"errors"
	"fmt"
	"sort"
	"strings"
)

const maxProviderErrorBodyBytes = 16 << 10

// ProviderErrorSummary returns a short user-facing provider failure message.
func ProviderErrorSummary(err error) string {
	if err == nil {
		return ""
	}
	return fmt.Sprintf("Provider error: %v", err)
}

// FormatProviderErrorDetail returns a multi-section log suitable for a detail box.
func FormatProviderErrorDetail(err error) string {
	if err == nil {
		return ""
	}

	var b strings.Builder
	b.WriteString("Provider request failed\n\n")
	b.WriteString(strings.TrimSpace(err.Error()))

	var pe *ProviderError
	if errors.As(err, &pe) && pe != nil {
		appendProviderErrorSections(&b, pe)
	}

	if cause := errors.Unwrap(err); cause != nil && cause.Error() != err.Error() {
		b.WriteString("\n\n--- Cause ---\n")
		b.WriteString(strings.TrimSpace(cause.Error()))
	}

	return strings.TrimRight(b.String(), "\n")
}

func appendProviderErrorSections(b *strings.Builder, pe *ProviderError) {
	if pe.Title != "" && pe.Title != pe.Message {
		b.WriteString("\n\n")
		b.WriteString("Title: ")
		b.WriteString(pe.Title)
	}
	if pe.StatusCode != 0 {
		b.WriteString("\n\nHTTP ")
		b.WriteString(fmt.Sprintf("%d", pe.StatusCode))
	}
	if pe.URL != "" {
		b.WriteString("\nURL: ")
		b.WriteString(pe.URL)
	}
	if pe.ContextTooLarge {
		b.WriteString("\nContext limit exceeded")
		if pe.ContextMaxTokens > 0 {
			b.WriteString(fmt.Sprintf(" (max %d", pe.ContextMaxTokens))
			if pe.ContextUsedTokens > 0 {
				b.WriteString(fmt.Sprintf(", used %d", pe.ContextUsedTokens))
			}
			b.WriteByte(')')
		}
	}
	if len(pe.ResponseHeaders) > 0 {
		b.WriteString("\n\n--- Response headers ---\n")
		keys := make([]string, 0, len(pe.ResponseHeaders))
		for key := range pe.ResponseHeaders {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		for _, key := range keys {
			b.WriteString(key)
			b.WriteString(": ")
			b.WriteString(pe.ResponseHeaders[key])
			b.WriteByte('\n')
		}
	}
	if body := formatProviderErrorBody("Request", pe.RequestBody); body != "" {
		b.WriteString("\n\n")
		b.WriteString(body)
	}
	if body := formatProviderErrorBody("Response", pe.ResponseBody); body != "" {
		b.WriteString("\n\n")
		b.WriteString(body)
	}
}

func formatProviderErrorBody(label string, raw []byte) string {
	trimmed := strings.TrimSpace(string(raw))
	if trimmed == "" {
		return ""
	}
	const header = "--- %s ---\n"
	if len(raw) <= maxProviderErrorBodyBytes {
		return fmt.Sprintf(header, label) + trimmed
	}
	truncated := strings.TrimSpace(string(raw[:maxProviderErrorBodyBytes]))
	return fmt.Sprintf(header, label) + truncated + fmt.Sprintf("\n\n[truncated — showing first %d bytes]", maxProviderErrorBodyBytes)
}
