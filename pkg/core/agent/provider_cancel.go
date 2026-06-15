package agent

import (
	"context"
	"errors"
	"strings"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/ai/utils"
)

// ProviderCancelError reports expected stream/turn cancellation failures that
// should not surface as provider error detail boxes.
func ProviderCancelError(err error) bool {
	if err == nil || isStreamStallFailure(err) {
		return false
	}
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return true
	}
	var pe *provider.ProviderError
	if errors.As(err, &pe) && pe != nil {
		if isStreamStallFailure(pe.Cause) || pe.Title == "stream stalled" {
			return false
		}
		if errors.Is(pe.Cause, context.Canceled) || errors.Is(pe.Cause, context.DeadlineExceeded) {
			return true
		}
		if pe.Title == "stream cancelled" {
			return true
		}
	}
	return false
}

func isStreamStallFailure(err error) bool {
	if err == nil {
		return false
	}
	if errors.Is(err, utils.ErrStreamStall) {
		return true
	}
	return strings.Contains(strings.ToLower(err.Error()), "stream stalled")
}
