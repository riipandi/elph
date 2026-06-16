package exec

import (
	"context"
	"fmt"
	"strings"

	"github.com/riipandi/elph/internal/runtime/toolresult"
	"github.com/riipandi/elph/pkg/tools/goal"
)

func executeCreateGoal(ctx context.Context, args map[string]any) toolresult.ToolResult {
	mgr := goal.FromContext(ctx)
	if mgr == nil {
		return toolresult.ToolResult{Err: fmt.Errorf("goal manager unavailable")}
	}

	objective, ok := stringArg(args, "objective")
	if !ok {
		return toolresult.ToolResult{Err: fmt.Errorf("missing required argument: objective")}
	}

	completionCriterion, _ := stringArg(args, "completionCriterion")
	replace := boolArg(args, "replace")

	snapshot, err := mgr.CreateGoal(objective, completionCriterion, replace)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}

	return toolresult.ToolResult{
		Output: fmt.Sprintf("Goal created: %s (status: %s)", snapshot.Objective, snapshot.Status),
	}
}

func executeGetGoal(ctx context.Context, _ map[string]any) toolresult.ToolResult {
	mgr := goal.FromContext(ctx)
	if mgr == nil {
		return toolresult.ToolResult{Err: fmt.Errorf("goal manager unavailable")}
	}

	snapshot := mgr.GetGoal()
	if snapshot == nil {
		return toolresult.ToolResult{Output: "No active goal."}
	}

	var b strings.Builder
	fmt.Fprintf(&b, "Objective: %s\n", snapshot.Objective)
	if snapshot.CompletionCriterion != "" {
		fmt.Fprintf(&b, "Completion criterion: %s\n", snapshot.CompletionCriterion)
	}
	fmt.Fprintf(&b, "Status: %s\n", snapshot.Status)
	if snapshot.TurnBudget > 0 {
		fmt.Fprintf(&b, "Turn budget: %d\n", snapshot.TurnBudget)
	}
	if snapshot.TokenBudget > 0 {
		fmt.Fprintf(&b, "Token budget: %d\n", snapshot.TokenBudget)
	}
	fmt.Fprintf(&b, "Turns used: %d\n", snapshot.TurnsUsed)
	fmt.Fprintf(&b, "Tokens used: %d\n", snapshot.TokensUsed)
	if snapshot.WallClockMs > 0 {
		fmt.Fprintf(&b, "Elapsed: %s\n", formatDuration(snapshot.WallClockMs))
	}
	if snapshot.WallClockBudgetMs > 0 {
		fmt.Fprintf(&b, "Wall clock budget: %s\n", formatDuration(snapshot.WallClockBudgetMs))
	}

	return toolresult.ToolResult{Output: b.String()}
}

func executeUpdateGoal(ctx context.Context, args map[string]any) toolresult.ToolResult {
	mgr := goal.FromContext(ctx)
	if mgr == nil {
		return toolresult.ToolResult{Err: fmt.Errorf("goal manager unavailable")}
	}

	statusRaw, ok := stringArg(args, "status")
	if !ok {
		return toolresult.ToolResult{Err: fmt.Errorf("missing required argument: status")}
	}

	status := goal.Status(strings.ToLower(statusRaw))
	snapshot, err := mgr.UpdateGoal(status)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}

	return toolresult.ToolResult{
		Output: fmt.Sprintf("Goal status updated to: %s", snapshot.Status),
	}
}

func executeSetGoalBudget(ctx context.Context, args map[string]any) toolresult.ToolResult {
	mgr := goal.FromContext(ctx)
	if mgr == nil {
		return toolresult.ToolResult{Err: fmt.Errorf("goal manager unavailable")}
	}

	value, ok := args["value"]
	if !ok || value == nil {
		return toolresult.ToolResult{Err: fmt.Errorf("missing required argument: value")}
	}
	unit, ok := stringArg(args, "unit")
	if !ok {
		return toolresult.ToolResult{Err: fmt.Errorf("missing required argument: unit")}
	}

	var intVal int64
	switch v := value.(type) {
	case float64:
		intVal = int64(v)
	case int:
		intVal = int64(v)
	case int64:
		intVal = v
	default:
		return toolresult.ToolResult{Err: fmt.Errorf("value must be numeric")}
	}

	if intVal <= 0 {
		return toolresult.ToolResult{Err: fmt.Errorf("value must be positive")}
	}

	var limits goal.BudgetLimits
	switch strings.ToLower(unit) {
	case "turns":
		if intVal <= 0 {
			return toolresult.ToolResult{Err: fmt.Errorf("turn budget must be positive")}
		}
		limits.TurnBudget = &intVal
	case "tokens":
		if intVal <= 0 {
			return toolresult.ToolResult{Err: fmt.Errorf("token budget must be positive")}
		}
		limits.TokenBudget = &intVal
	case "milliseconds":
		if intVal < 1000 || intVal > 86400000 {
			return toolresult.ToolResult{Err: fmt.Errorf("time budget must be between 1 second and 24 hours")}
		}
		limits.WallClockBudget = &intVal
	case "seconds":
		ms := intVal * 1000
		if ms < 1000 || ms > 86400000 {
			return toolresult.ToolResult{Err: fmt.Errorf("time budget must be between 1 second and 24 hours")}
		}
		limits.WallClockBudget = &ms
	case "minutes":
		ms := intVal * 60 * 1000
		if ms < 1000 || ms > 86400000 {
			return toolresult.ToolResult{Err: fmt.Errorf("time budget must be between 1 second and 24 hours")}
		}
		limits.WallClockBudget = &ms
	case "hours":
		ms := intVal * 60 * 60 * 1000
		if ms < 1000 || ms > 86400000 {
			return toolresult.ToolResult{Err: fmt.Errorf("time budget must be between 1 second and 24 hours")}
		}
		limits.WallClockBudget = &ms
	default:
		return toolresult.ToolResult{Err: fmt.Errorf("unsupported budget unit: %s (use turns, tokens, milliseconds, seconds, minutes, or hours)", unit)}
	}

	mgr.SetBudgetLimits(limits)
	return toolresult.ToolResult{
		Output: fmt.Sprintf("Goal budget set: %d %s", intVal, unit),
	}
}

func formatDuration(ms int64) string {
	if ms < 1000 {
		return fmt.Sprintf("%dms", ms)
	}
	totalSeconds := ms / 1000
	if totalSeconds < 60 {
		return fmt.Sprintf("%ds", totalSeconds)
	}
	minutes := totalSeconds / 60
	seconds := totalSeconds % 60
	if minutes < 60 {
		return fmt.Sprintf("%dm%ds", minutes, seconds)
	}
	hours := minutes / 60
	minutes = minutes % 60
	return fmt.Sprintf("%dh%dm", hours, minutes)
}
