package prompt

import (
	_ "embed"
	"strings"
	"text/template"
)

//go:embed stubs/system.md
var builtinSystemPrompt string

var builtinTemplate = template.Must(
	template.New("builtin").Parse(builtinSystemPrompt),
)

func renderSystemPrompt(source string, data TemplateData) string {
	tmpl, err := template.New("system").Parse(source)
	if err != nil {
		return ""
	}

	var b strings.Builder
	if err := tmpl.Execute(&b, data); err != nil {
		return ""
	}
	return strings.TrimSpace(b.String())
}

func renderBuiltinSystemPrompt(data TemplateData) string {
	var b strings.Builder
	if err := builtinTemplate.Execute(&b, data); err != nil {
		return ""
	}
	return strings.TrimSpace(b.String())
}
