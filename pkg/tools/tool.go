// Package tool defines built-in coding-agent tools as a publishable library.
//
// Layout:
//
//	catalog/   — tool names, categories, and the built-in catalog
//	exposure/  — name resolution and runtime executability
//	schema/    — provider API JSON schemas and exposure filter
//	websearch/ — reusable multi-engine web search
//	todolist/  — TodoList tool state and argument handling
//
// Tool execution and approval UI live in internal/ (coding-agent).
package tools

import (
	"github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/riipandi/elph/pkg/tools/catalog"
	"github.com/riipandi/elph/pkg/tools/exposure"
	"github.com/riipandi/elph/pkg/tools/schema"
)

type (
	Category   = catalog.Category
	Approval   = catalog.Approval
	Definition = catalog.Definition
)

const (
	CategoryFile            = catalog.CategoryFile
	CategoryShell           = catalog.CategoryShell
	CategoryWeb             = catalog.CategoryWeb
	CategoryPlanMode        = catalog.CategoryPlanMode
	CategoryStateManagement = catalog.CategoryStateManagement
	CategoryCollaboration   = catalog.CategoryCollaboration
	CategoryGoal            = catalog.CategoryGoal

	ApprovalAutoAllow        = catalog.ApprovalAutoAllow
	ApprovalRequiresApproval = catalog.ApprovalRequiresApproval
	ApprovalAlwaysApprove    = catalog.ApprovalAlwaysApprove

	Read          = catalog.Read
	Write         = catalog.Write
	Edit          = catalog.Edit
	Grep          = catalog.Grep
	Glob          = catalog.Glob
	ReadMediaFile = catalog.ReadMediaFile
	Bash          = catalog.Bash
	FetchURL      = catalog.FetchURL
	WebSearch     = catalog.WebSearch
	CodeSearch    = catalog.CodeSearch
	EnterPlanMode = catalog.EnterPlanMode
	ExitPlanMode  = catalog.ExitPlanMode
	AskUser       = catalog.AskUser
	Skill         = catalog.Skill
	TodoList      = catalog.TodoList
	CreateGoal    = catalog.CreateGoal
	GetGoal       = catalog.GetGoal
	UpdateGoal    = catalog.UpdateGoal
	SetGoalBudget = catalog.SetGoalBudget
)

func Get(name string) (Definition, bool)                    { return catalog.Get(name) }
func All() []Definition                                     { return catalog.All() }
func ByCategory(category Category) []Definition             { return catalog.ByCategory(category) }
func Names() []string                                       { return catalog.Names() }
func RequiresApproval(name string) bool                     { return catalog.RequiresApproval(name) }
func ResolveName(raw string) (canonical string, known bool) { return exposure.ResolveName(raw) }
func IsExecutable(name string) bool                         { return exposure.IsExecutable(name) }
func IsProviderExposed(name string) bool                    { return schema.IsProviderExposed(name) }
func ProviderDefinitions() []protocol.ToolDefinition        { return schema.ProviderDefinitions() }
func FilterProviderTools(tools []protocol.ToolDefinition) []protocol.ToolDefinition {
	return schema.FilterProviderTools(tools)
}
