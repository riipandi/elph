package renderer

import (
	"errors"

	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/runtime"
)

func toolDetailStatus(result runtime.ToolResult) constants.DetailStatus {
	if result.Cancelled {
		return constants.DetailStatusWarning
	}
	if result.Err != nil {
		switch {
		case errors.Is(result.Err, runtime.ErrToolUnknown):
			return constants.DetailStatusError
		case errors.Is(result.Err, runtime.ErrToolUnavailable):
			return constants.DetailStatusUnavailable
		default:
			return constants.DetailStatusError
		}
	}
	return constants.DetailStatusSuccess
}

func toolRequestDetailStatus(reason runtime.UnavailableReason) constants.DetailStatus {
	switch reason {
	case runtime.UnavailableUnknown:
		return constants.DetailStatusError
	default:
		return constants.DetailStatusUnavailable
	}
}

func shellDetailStatus(result *runtime.ShellResult, running bool) constants.DetailStatus {
	if running || result == nil {
		return constants.DetailStatusRunning
	}
	if result.Cancelled {
		return constants.DetailStatusWarning
	}
	if result.Err != nil || result.ExitCode != 0 {
		return constants.DetailStatusError
	}
	return constants.DetailStatusSuccess
}
