package command

import (
	"fmt"
	"strings"

	"github.com/riipandi/elph/pkg/skill"
)

const skillCommandPrefix = "skill:"

// SlashSkill is a registered agentskills.io entry exposed as /skill:<name>.
type SlashSkill struct {
	Name         string
	Description  string
	ArgumentHint string
}

func skillCommand(def SlashSkill) SlashCommand {
	return SlashCommand{
		Name:         skillCommandPrefix + def.Name,
		Description:  def.Description,
		ArgumentHint: def.ArgumentHint,
		Skill:        true,
	}
}

func skillCommands(defs []SlashSkill) []SlashCommand {
	out := make([]SlashCommand, 0, len(defs))
	for _, def := range defs {
		out = append(out, skillCommand(def))
	}
	return out
}

func skillNameFromCommand(cmdName string) (string, bool) {
	if !strings.HasPrefix(strings.ToLower(cmdName), skillCommandPrefix) {
		return "", false
	}
	name := strings.TrimSpace(cmdName[len(skillCommandPrefix):])
	if name == "" {
		return "", false
	}
	return name, true
}

// LoadSlashSkills discovers skills for /skill:<name> commands.
func LoadSlashSkills(workDir string) []SlashSkill {
	defs := skill.DiscoverAll(workDir)
	out := make([]SlashSkill, len(defs))
	for i, def := range defs {
		out[i] = SlashSkill{
			Name:         def.Name,
			Description:  def.Description,
			ArgumentHint: def.ArgumentHint,
		}
	}
	return out
}

type skillSlashExpansion struct {
	AgentPrompt string
	DetailLabel string
	DetailBody  string
}

func expandSkillSlash(ctx Context, cmd SlashCommand, args string) (skillSlashExpansion, error) {
	skillName, ok := skillNameFromCommand(cmd.Name)
	if !ok {
		return skillSlashExpansion{}, fmt.Errorf("invalid skill command /%s", cmd.Name)
	}
	def, err := skill.Resolve(ctx.WorkDir, skillName)
	if err != nil {
		return skillSlashExpansion{}, err
	}
	if err := skill.ValidateName(def.Name); err != nil {
		return skillSlashExpansion{}, fmt.Errorf("invalid skill %q: %w", def.Name, err)
	}
	if err := skill.ValidateDescription(def.Description); err != nil {
		return skillSlashExpansion{}, fmt.Errorf("invalid skill %q: %w", def.Name, err)
	}
	agentPrompt := skill.SlashAgentPrompt(def, args)
	if strings.TrimSpace(agentPrompt) == "" {
		return skillSlashExpansion{}, fmt.Errorf("skill %q is empty", skillName)
	}
	return skillSlashExpansion{
		AgentPrompt: agentPrompt,
		DetailLabel: skill.SlashDetailLabel(def.Name),
		DetailBody:  skill.SlashDetailBody(def, args),
	}, nil
}
