package command

import (
	"fmt"
	"strings"
)

func goalHandler(ctx *Context, args string) string {
	if ctx.GoalManager == nil {
		return "Goal manager unavailable."
	}

	subcommand, rest := parseGoalSubcommand(args)

	switch subcommand {
	case "":
		return goalStatus(ctx)
	case "status":
		return goalStatus(ctx)
	case "pause":
		return goalPause(ctx)
	case "resume":
		return goalResume(ctx)
	case "cancel":
		return goalCancel(ctx)
	case "replace":
		return goalReplace(ctx, rest)
	case "next":
		return goalNext(ctx, rest)
	default:
		// If no subcommand matches, treat the entire args as an objective
		// to create a new goal (like Kimi Code: "/goal <objective>")
		return goalCreateFromArgs(ctx, args)
	}
}

func parseGoalSubcommand(args string) (subcommand, rest string) {
	args = strings.TrimSpace(args)
	if args == "" {
		return "", ""
	}
	parts := strings.SplitN(args, " ", 2)
	subcommand = strings.ToLower(parts[0])
	if len(parts) == 2 {
		rest = strings.TrimSpace(parts[1])
	}
	return subcommand, rest
}

func goalStatus(ctx *Context) string {
	gm := ctx.GoalManager
	if gm == nil {
		return "Goal manager unavailable."
	}
	s := gm.GetGoal()
	if s == nil {
		return "No active goal.\nUse /goal <objective> to create one, or see /goal replace <objective>."
	}
	var b strings.Builder
	fmt.Fprintf(&b, "Objective: %s\n", s.Objective)
	if s.CompletionCriterion != "" {
		fmt.Fprintf(&b, "Completion criterion: %s\n", s.CompletionCriterion)
	}
	fmt.Fprintf(&b, "Status: %s\n", s.Status)
	fmt.Fprintf(&b, "Turns used: %d\n", s.TurnsUsed)
	fmt.Fprintf(&b, "Tokens used: %d\n", s.TokensUsed)
	if s.WallClockMs > 0 {
		fmt.Fprintf(&b, "Elapsed: %s\n", fmtDuration(s.WallClockMs))
	}
	if s.WallClockBudgetMs > 0 {
		fmt.Fprintf(&b, "Wall clock budget: %s\n", fmtDuration(s.WallClockBudgetMs))
	}
	if s.TurnBudget > 0 {
		fmt.Fprintf(&b, "Turn budget: %d\n", s.TurnBudget)
	}
	if s.TokenBudget > 0 {
		fmt.Fprintf(&b, "Token budget: %d\n", s.TokenBudget)
	}
	return b.String()
}

func goalPause(ctx *Context) string {
	gm := ctx.GoalManager
	s := gm.GetGoal()
	if s == nil {
		return "No active goal to pause."
	}
	if s.Status != "active" {
		return fmt.Sprintf("Goal is not active (status: %s). Only active goals can be paused.", s.Status)
	}
	_, err := gm.UpdateGoal("paused")
	if err != nil {
		return fmt.Sprintf("Failed to pause goal: %s", err)
	}
	return "Goal paused."
}

func goalResume(ctx *Context) string {
	gm := ctx.GoalManager
	s := gm.GetGoal()
	if s == nil {
		return "No saved goal to resume."
	}
	if s.Status != "paused" && s.Status != "blocked" {
		return fmt.Sprintf("Goal is not paused or blocked (status: %s). Only paused or blocked goals can be resumed.", s.Status)
	}
	_, err := gm.UpdateGoal("active")
	if err != nil {
		return fmt.Sprintf("Failed to resume goal: %s", err)
	}
	return "Goal resumed."
}

func goalCancel(ctx *Context) string {
	gm := ctx.GoalManager
	s := gm.GetGoal()
	if s == nil {
		return "No active goal to cancel."
	}
	gm.Clear()
	return "Goal cancelled."
}

func goalReplace(ctx *Context, objective string) string {
	if strings.TrimSpace(objective) == "" {
		return "Usage: /goal replace <objective>"
	}
	gm := ctx.GoalManager
	snapshot, err := gm.CreateGoal(objective, "", true) // replace=true
	if err != nil {
		return fmt.Sprintf("Failed to replace goal: %s", err)
	}
	return fmt.Sprintf("Goal replaced: %s (status: %s)", snapshot.Objective, snapshot.Status)
}

func goalNext(ctx *Context, objective string) string {
	if strings.TrimSpace(objective) == "" {
		return "Usage: /goal next <objective>\nQueue an upcoming goal. If no goal is active, it starts immediately."
	}
	gm := ctx.GoalManager
	s := gm.GetGoal()
	if s != nil && s.Status != "complete" {
		return "A goal is already active. Wait until it completes, then use /goal replace to replace it."
	}
	// No pending goal queue yet - start the goal directly
	snapshot, err := gm.CreateGoal(objective, "", false)
	if err != nil {
		return fmt.Sprintf("Failed to queue goal: %s", err)
	}
	return fmt.Sprintf("Goal started: %s (status: %s)", snapshot.Objective, snapshot.Status)
}

func goalCreateFromArgs(ctx *Context, args string) string {
	if strings.TrimSpace(args) == "" {
		return "Usage: /goal <objective>\nExample: /goal Update the checkout docs"
	}
	gm := ctx.GoalManager
	s := gm.GetGoal()
	if s != nil && s.Status != "complete" {
		return fmt.Sprintf("A goal is already active (status: %s).\nUse /goal replace <objective> to replace it, or /goal cancel first.", s.Status)
	}
	snapshot, err := gm.CreateGoal(args, "", false)
	if err != nil {
		return fmt.Sprintf("Failed to create goal: %s", err)
	}
	return fmt.Sprintf("Goal created: %s (status: %s)", snapshot.Objective, snapshot.Status)
}

func fmtDuration(ms int64) string {
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
