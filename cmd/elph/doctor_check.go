package main

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/riipandi/elph/internal/projectdir"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai"
	"github.com/riipandi/elph/pkg/ai/provider"
)

type doctorStatus int

const (
	doctorOK doctorStatus = iota
	doctorWarn
	doctorFail
)

type doctorFinding struct {
	Status doctorStatus
	Label  string
	Detail string
}

type doctorReport struct {
	Findings []doctorFinding
}

func (r *doctorReport) add(status doctorStatus, label, detail string) {
	r.Findings = append(r.Findings, doctorFinding{
		Status: status,
		Label:  label,
		Detail: detail,
	})
}

func (r doctorReport) hasFailures() bool {
	for _, f := range r.Findings {
		if f.Status == doctorFail {
			return true
		}
	}
	return false
}

func (r doctorReport) counts() (ok, warn, fail int) {
	for _, f := range r.Findings {
		switch f.Status {
		case doctorOK:
			ok++
		case doctorWarn:
			warn++
		case doctorFail:
			fail++
		}
	}
	return ok, warn, fail
}

func (r doctorReport) write(w io.Writer) {
	fmt.Fprintln(w, "Elph doctor")
	fmt.Fprintln(w)

	current := ""
	for _, f := range r.Findings {
		if f.Label != current {
			if current != "" {
				fmt.Fprintln(w)
			}
			fmt.Fprintf(w, "%s\n", f.Label)
			current = f.Label
		}
		fmt.Fprintf(w, "  %s  %s\n", doctorStatusTag(f.Status), f.Detail)
	}

	ok, warn, fail := r.counts()
	fmt.Fprintln(w)
	switch {
	case fail > 0:
		fmt.Fprintf(w, "Summary: %d ok, %d warning(s), %d error(s)\n", ok, warn, fail)
	case warn > 0:
		fmt.Fprintf(w, "Summary: %d ok, %d warning(s)\n", ok, warn)
	default:
		fmt.Fprintf(w, "Summary: %d ok\n", ok)
	}
}

func doctorStatusTag(status doctorStatus) string {
	switch status {
	case doctorOK:
		return "ok "
	case doctorWarn:
		return "warn"
	default:
		return "fail"
	}
}

func (r *doctorReport) checkEnvironment() {
	section := "Environment"
	overrides := []string{
		envOverride("ELPH_PROVIDERS_DIR", os.Getenv("ELPH_PROVIDERS_DIR")),
		envOverride("ELPH_PROVIDER", os.Getenv("ELPH_PROVIDER")),
		envOverride("ELPH_MODEL", os.Getenv("ELPH_MODEL")),
	}
	overrides = compactStrings(overrides)
	if len(overrides) == 0 {
		r.add(doctorOK, section, "no ELPH_* overrides")
		return
	}
	for _, line := range overrides {
		r.add(doctorOK, section, line)
	}
}

func envOverride(name, value string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return ""
	}
	return fmt.Sprintf("%s=%s", name, value)
}

func (r *doctorReport) checkSettings(workDir string) error {
	section := "Settings"

	path, err := settings.Path()
	if err != nil {
		return err
	}
	if _, statErr := os.Stat(path); statErr != nil {
		if os.IsNotExist(statErr) {
			r.add(doctorWarn, section, fmt.Sprintf("%s missing (defaults will be used)", tildePath(path)))
		} else {
			r.add(doctorFail, section, fmt.Sprintf("%s: %v", tildePath(path), err))
		}
	} else {
		r.add(doctorOK, section, tildePath(path))
	}

	cfg, err := settings.LoadFor(workDir)
	if err != nil {
		r.add(doctorFail, section, fmt.Sprintf("load failed: %v", err))
		return nil
	}

	interval := strings.TrimSpace(cfg.SyncInterval)
	if interval == "" && cfg.Models != nil {
		interval = strings.TrimSpace(cfg.Models.SyncInterval)
	}
	if interval == "" {
		r.add(doctorOK, section, "syncInterval: 24h (default)")
	} else if d, parseErr := time.ParseDuration(interval); parseErr != nil || d <= 0 {
		r.add(doctorWarn, section, fmt.Sprintf("syncInterval %q is invalid (using 24h)", interval))
	} else {
		r.add(doctorOK, section, fmt.Sprintf("syncInterval: %s", interval))
	}

	projectPath := projectSettingsPath(workDir)
	if projectPath == "" {
		return nil
	}
	if _, err := os.Stat(projectPath); err == nil {
		r.add(doctorOK, section, fmt.Sprintf("project overrides: %s", tildePath(projectPath)))
	}
	return nil
}

func projectSettingsPath(workDir string) string {
	workDir = strings.TrimSpace(workDir)
	if workDir == "" {
		return ""
	}
	root := projectdir.Root(workDir)
	for _, name := range []string{"settings.json", "settings.jsonc"} {
		path := filepath.Join(root, name)
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}
	return ""
}

func (r *doctorReport) checkVersion() error {
	section := "Version metadata"

	path, err := settings.VersionPath()
	if err != nil {
		return err
	}

	v, err := settings.LoadVersion()
	if err != nil {
		r.add(doctorFail, section, fmt.Sprintf("%s: %v", tildePath(path), err))
		return nil
	}

	if _, err := os.Stat(path); os.IsNotExist(err) {
		r.add(doctorOK, section, fmt.Sprintf("%s will be created on first provider sync", tildePath(path)))
	} else if err != nil {
		r.add(doctorFail, section, fmt.Sprintf("%s: %v", tildePath(path), err))
		return nil
	} else {
		r.add(doctorOK, section, tildePath(path))
	}

	if last, ok := v.LastSyncProvidersTime(); ok {
		r.add(doctorOK, section, fmt.Sprintf("last provider sync: %s", last.Format(time.RFC3339)))
	} else {
		r.add(doctorWarn, section, "provider metadata never synced — run: elph provider update")
	}
	return nil
}

func (r *doctorReport) checkProviders() error {
	section := "Providers"

	dir, err := provider.ProvidersDir()
	if err != nil {
		return err
	}

	info, err := os.Stat(dir)
	if err != nil {
		if os.IsNotExist(err) {
			r.add(doctorFail, section, fmt.Sprintf("%s missing — run: elph provider connect", tildePath(dir)))
			return nil
		}
		r.add(doctorFail, section, fmt.Sprintf("%s: %v", tildePath(dir), err))
		return nil
	}
	if !info.IsDir() {
		r.add(doctorFail, section, fmt.Sprintf("%s is not a directory", tildePath(dir)))
		return nil
	}
	r.add(doctorOK, section, tildePath(dir))

	catalog, err := provider.LoadCatalog("")
	if err != nil {
		r.add(doctorFail, section, fmt.Sprintf("load failed: %v", err))
		return nil
	}

	for _, loadErr := range catalog.Errors {
		r.add(doctorFail, section, loadErr.Error())
	}

	if len(catalog.Providers) == 0 {
		r.add(doctorFail, section, "no provider files found — run: elph provider connect")
		return nil
	}

	enabledProviders := 0
	enabledModels := 0
	readyProviders := 0
	for _, reg := range catalog.Providers {
		if !provider.ProviderConfigEnabled(reg.Config) {
			continue
		}
		enabledProviders++
		enabledModels += provider.EnabledModelCount(reg)
		if provider.IsConfigured(reg.Config.APIKey) && providerCredentialReady(reg) {
			readyProviders++
		}
	}

	r.add(doctorOK, section, fmt.Sprintf(
		"%d provider file(s), %d enabled, %d enabled model(s), %d with working credentials",
		len(catalog.Providers), enabledProviders, enabledModels, readyProviders,
	))

	if enabledModels == 0 {
		r.add(doctorFail, section, "no enabled models — run: elph provider model list")
	}
	return nil
}

func providerCredentialReady(reg provider.RegisteredProvider) bool {
	if !provider.IsConfigured(reg.Config.APIKey) {
		return false
	}
	if provider.EnabledModelCount(reg) == 0 {
		return false
	}
	model, ok := provider.FirstEnabledModel(reg)
	if !ok {
		return false
	}
	_, err := provider.BuildModelConfig(provider.Catalog{Providers: []provider.RegisteredProvider{reg}}, reg, model)
	return err == nil
}

func (r *doctorReport) checkActiveModel() {
	section := "Active model"

	prefs, err := settings.Load()
	if err != nil {
		r.add(doctorFail, section, fmt.Sprintf("load settings: %v", err))
		return
	}

	savedProvider := prefs.ActiveProviderID()
	savedModel := prefs.ActiveModelID()
	if savedProvider == "" && savedModel == "" {
		r.add(doctorWarn, section, "no saved model in ~/.elph/settings.json")
	} else {
		r.add(doctorOK, section, fmt.Sprintf("saved selection: %s/%s", displayID(savedProvider), displayID(savedModel)))
		r.checkSavedModelCredentials(section, savedProvider, savedModel)
	}

	cfg := ai.ResolveProvider(savedProvider, savedModel)
	if cfg.ProviderID == "" || cfg.ModelID == "" {
		if savedProvider != "" || savedModel != "" {
			r.add(doctorWarn, section, "saved model is not usable (disabled, missing, or credentials unavailable)")
		} else {
			r.add(doctorWarn, section, "no runtime model — pick one in the TUI (Ctrl+L) or set ELPH_PROVIDER/ELPH_MODEL")
		}
		return
	}

	name := cfg.ModelName
	if name == "" {
		name = cfg.ModelID
	}
	providerName := cfg.ProviderName
	if providerName == "" {
		providerName = cfg.ProviderID
	}
	r.add(doctorOK, section, fmt.Sprintf("runtime: %s/%s (%s · %s)", cfg.ProviderID, cfg.ModelID, providerName, name))

	if cfg.Provider == nil {
		reg, ok := cfg.Catalog.Provider(cfg.ProviderID)
		if !ok {
			r.add(doctorFail, section, fmt.Sprintf("provider %q not found in catalog", cfg.ProviderID))
			return
		}
		model, ok := cfg.Catalog.Model(cfg.ProviderID, cfg.ModelID)
		if !ok {
			r.add(doctorFail, section, fmt.Sprintf("model %q not found for provider %q", cfg.ModelID, cfg.ProviderID))
			return
		}
		_, credErr := provider.BuildModelConfig(cfg.Catalog, reg, model)
		if credErr != nil {
			if provider.IsCredentialError(credErr) {
				r.add(doctorWarn, section, provider.CredentialHint(reg))
			} else {
				r.add(doctorFail, section, credErr.Error())
			}
		}
		return
	}

	r.add(doctorOK, section, "credentials verified for runtime model")
}

func (r *doctorReport) checkSavedModelCredentials(section, providerID, modelID string) {
	providerID = strings.TrimSpace(providerID)
	modelID = strings.TrimSpace(modelID)
	if providerID == "" {
		return
	}

	catalog, err := provider.LoadCatalog("")
	if err != nil {
		r.add(doctorFail, section, fmt.Sprintf("load providers: %v", err))
		return
	}
	reg, ok := catalog.Provider(providerID)
	if !ok {
		r.add(doctorFail, section, fmt.Sprintf("saved provider %q not found", providerID))
		return
	}
	if !provider.ProviderConfigEnabled(reg.Config) {
		r.add(doctorWarn, section, fmt.Sprintf("saved provider %q is disabled", providerID))
		return
	}

	model, ok := pickSavedModel(reg, modelID)
	if !ok {
		r.add(doctorFail, section, fmt.Sprintf("saved model %q not found for provider %q", displayID(modelID), providerID))
		return
	}
	if !model.Enabled {
		r.add(doctorWarn, section, fmt.Sprintf("saved model %q is disabled", model.ID))
	}

	_, credErr := provider.BuildModelConfig(catalog, reg, model)
	if credErr == nil {
		return
	}
	if provider.IsCredentialError(credErr) {
		r.add(doctorWarn, section, provider.CredentialHint(reg))
		return
	}
	r.add(doctorFail, section, credErr.Error())
}

func pickSavedModel(reg provider.RegisteredProvider, modelID string) (provider.ResolvedModel, bool) {
	modelID = strings.TrimSpace(modelID)
	if modelID != "" {
		for _, model := range reg.Models {
			if model.ID == modelID || model.Name == modelID {
				return model, true
			}
		}
		return provider.ResolvedModel{}, false
	}
	return provider.FirstEnabledModel(reg)
}

func displayID(id string) string {
	id = strings.TrimSpace(id)
	if id == "" {
		return "—"
	}
	return id
}

func tildePath(path string) string {
	path = strings.TrimSpace(path)
	if path == "" {
		return path
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return path
	}
	if rel, err := filepath.Rel(home, path); err == nil && !strings.HasPrefix(rel, "..") {
		return filepath.Join("~", rel)
	}
	return path
}

func compactStrings(items []string) []string {
	out := make([]string, 0, len(items))
	for _, item := range items {
		if strings.TrimSpace(item) != "" {
			out = append(out, item)
		}
	}
	return out
}
