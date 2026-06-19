package renderer

import (
	"errors"

	"github.com/riipandi/elph/internal/runtime/shell"
	"github.com/riipandi/elph/internal/runtime/toolresult"

	"github.com/riipandi/elph/internal/uiconst"
)

func bashToolDetailStatus(result toolresult.ToolResult) uiconst.DetailStatus {
	if result.Cancelled {
		return uiconst.DetailStatusWarning
	}
	if result.Err != nil {
		return uiconst.DetailStatusError
	}
	if _, exitCode := shell.SplitShellExitSuffix(result.Output); exitCode != 0 {
		return uiconst.DetailStatusError
	}
	return uiconst.DetailStatusSuccess
}

func toolDetailStatus(result toolresult.ToolResult) uiconst.DetailStatus {
	if result.Cancelled {
		return uiconst.DetailStatusWarning
	}
	if result.Err != nil {
		switch {
		case errors.Is(result.Err, toolresult.ErrToolUnknown):
			return uiconst.DetailStatusError
		case errors.Is(result.Err, toolresult.ErrToolUnavailable):
			return uiconst.DetailStatusUnavailable
		default:
			return uiconst.DetailStatusError
		}
	}
	return uiconst.DetailStatusSuccess
}

func toolRequestDetailStatus(reason toolresult.UnavailableReason) uiconst.DetailStatus {
	switch reason {
	case toolresult.UnavailableUnknown:
		return uiconst.DetailStatusError
	default:
		return uiconst.DetailStatusUnavailable
	}
}

func shellDetailStatus(result *shell.ShellResult, running bool) uiconst.DetailStatus {
	if running || result == nil {
		return uiconst.DetailStatusRunning
	}
	if result.Cancelled {
		return uiconst.DetailStatusWarning
	}
	if result.Err != nil || result.ExitCode != 0 {
		return uiconst.DetailStatusError
	}
	return uiconst.DetailStatusSuccess
}
