package renderer

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/align"
	"github.com/riipandi/elph/internal/command"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai"
	"github.com/riipandi/elph/pkg/ai/provider"
)

const (
	modelSelectorVisibleModels = 6
	modelSelectorPlaceholder   = "Filter models..."
	modelSelectorCurrentMark   = "‹ "
	modelSelectorCurrentLabel  = "current"
)

var modelSelectorCurrentMarker = lipgloss.NewStyle().Foreground(constants.Green)

// ModelSelectorState tracks the interactive /model picker overlay.
type ModelSelectorState struct {
	Active           bool
	Query            string
	ProviderFilterID string
	Groups           []ai.SelectorGroup
	Flat             []provider.ResolvedModel
	Selected         int
	Scroll           int
	Catalog          provider.Catalog
}

func (m Model) modelSelectorActive() bool {
	return m.modelSelector.Active
}

func (m Model) hasActiveModel() bool {
	if m.session.Provider != nil {
		return true
	}
	return m.session.ProviderID != "" && m.session.ModelID != ""
}

func isModelSlashInput(raw string) bool {
	trimmed := strings.TrimSpace(raw)
	if !isSlashCommand(trimmed) {
		return false
	}
	body := strings.TrimSpace(strings.TrimPrefix(strings.TrimLeft(trimmed, " \t"), "/"))
	if body == "" {
		return false
	}
	name := strings.ToLower(strings.SplitN(body, " ", 2)[0])
	return name == "model"
}

func (m Model) modelPickerCatalog() (provider.Catalog, error) {
	if _, _, err := provider.EnsureStarterProviders(); err != nil {
		return provider.Catalog{}, err
	}
	catalog, err := provider.LoadCatalog("")
	if err != nil {
		return provider.Catalog{}, err
	}
	if len(catalog.Providers) == 0 {
		catalog = m.session.Catalog
	}
	return catalog, nil
}

func (m Model) applyModelPickerCatalog(catalog provider.Catalog) Model {
	m.session.Catalog = catalog
	m.session.EnabledModelCount = catalog.TotalEnabledModels()
	return m
}

func (m Model) promptSelectModel() (Model, tea.Cmd) {
	catalog, err := m.modelPickerCatalog()
	if err != nil {
		return m.withMessage(fmt.Sprintf("No model selected — %v", err))
	}
	if len(catalog.Providers) == 0 {
		return m.withMessage("No providers configured — run: elph provider connect")
	}
	if catalog.TotalEnabledModels() == 0 {
		return m.withMessage("No enabled models — run: elph provider model list")
	}
	m = m.applyModelPickerCatalog(catalog)
	m, cmd := m.withMessage("Select a model first (Ctrl+L)")
	m = m.openModelSelectorPreservingDraft(catalog)
	return m, cmd
}

func (m Model) triggerModelSelector() (Model, tea.Cmd) {
	if m.modelSelectorActive() {
		m = m.closeModelSelector()
		m = m.syncLayout(m.content.AtBottom())
		return m, nil
	}

	catalog, err := m.modelPickerCatalog()
	if err != nil {
		return m.withMessage(fmt.Sprintf("/model: %v", err))
	}
	if len(catalog.Providers) == 0 {
		return m.withMessage("/model: no providers configured — run: elph provider connect")
	}

	m = m.applyModelPickerCatalog(catalog)
	m = m.openModelSelectorPreservingDraft(catalog)
	return m, nil
}

func (m Model) openModelSelector(catalog provider.Catalog, query string) Model {
	m.pendingPromptDraft = nil
	return m.openModelSelectorFiltered(catalog, query)
}

func (m Model) openModelSelectorPreservingDraft(catalog provider.Catalog) Model {
	m = m.stashPromptDraftIfNeeded()
	return m.openModelSelectorFiltered(catalog, "")
}

func (m Model) stashPromptDraftIfNeeded() Model {
	if isModelSlashInput(m.input.Value()) {
		m.pendingPromptDraft = nil
		return m
	}
	hasDraft := strings.TrimSpace(m.input.Value()) != "" ||
		len(m.pendingAttachments) > 0 ||
		len(m.inputPastes) > 0
	if !hasDraft {
		m.pendingPromptDraft = nil
		return m
	}
	pastes := make(map[int]string, len(m.inputPastes))
	for k, v := range m.inputPastes {
		pastes[k] = v
	}
	m.pendingPromptDraft = &promptDraftState{
		value:       m.input.Value(),
		pastes:      pastes,
		attachments: append([]inputAttachment(nil), m.pendingAttachments...),
	}
	return m
}

func (m Model) restorePromptDraft() Model {
	draft := m.pendingPromptDraft
	m.pendingPromptDraft = nil
	if draft == nil {
		m.input.SetValue("")
		m = m.clearInputPastes()
		m.pendingAttachments = nil
		m.input.SetHeight(1)
		return m.syncInputHeight()
	}
	m.input.SetValue(draft.value)
	if len(draft.pastes) > 0 {
		m.inputPastes = draft.pastes
	} else {
		m = m.clearInputPastes()
	}
	m.pendingAttachments = append([]inputAttachment(nil), draft.attachments...)
	m.input.SetHeight(1)
	return m.syncInputHeight()
}

func (m Model) openModelSelectorFiltered(catalog provider.Catalog, query string) Model {
	groups, flat := ai.BuildSelectorGroups(catalog, query)
	m.modelSelector = ModelSelectorState{
		Active:           true,
		Query:            query,
		ProviderFilterID: "",
		Groups:           groups,
		Flat:             flat,
		Selected:         ai.SelectorPickIndex(flat, m.session.ProviderID, m.session.ModelID),
		Catalog:          catalog,
	}
	m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
	m.input.SetValue(query)
	m.input.Placeholder = m.modelSelectorPlaceholderText()
	m.input.SetHeight(1)
	m.layout.InputScrollTop = 0
	m.showPromptPrefix = false
	m.input.Focus()
	return m.syncLayout(m.content.AtBottom())
}

func (m Model) closeModelSelector() Model {
	m.modelSelector = ModelSelectorState{}
	m.input.Placeholder = ""
	m.showPromptPrefix = false
	m = m.restorePromptDraft()
	m.input.Focus()
	return m
}

func (m Model) refreshModelSelectorItems() Model {
	prev := m.modelSelector.selectedModel()
	m.modelSelector.Query = m.input.Value()
	groups, _ := ai.BuildSelectorGroups(m.modelSelector.Catalog, m.modelSelector.Query)
	m.modelSelector.Groups = groups
	m.modelSelector.ProviderFilterID = ai.NormalizeProviderFilter(m.modelSelector.ProviderFilterID, groups)
	flat := ai.FlattenSelectorGroups(groups, m.modelSelector.ProviderFilterID)

	selected := m.modelSelector.Selected
	if prev.ProviderID != "" {
		selected = ai.SelectorPickIndex(flat, prev.ProviderID, prev.ID)
	} else if selected >= len(flat) {
		if len(flat) > 0 {
			selected = len(flat) - 1
		} else {
			selected = 0
		}
	}

	m.modelSelector.Flat = flat
	m.modelSelector.Selected = selected
	m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
	m.input.Placeholder = m.modelSelectorPlaceholderText()
	return m
}

func (m Model) cycleModelSelectorProvider(delta int) Model {
	if len(m.modelSelector.Groups) == 0 {
		return m
	}

	prev := m.modelSelector.selectedModel()
	m.modelSelector.ProviderFilterID = ai.CycleProviderFilter(m.modelSelector.ProviderFilterID, delta, m.modelSelector.Groups)
	m.modelSelector.Flat = ai.FlattenSelectorGroups(m.modelSelector.Groups, m.modelSelector.ProviderFilterID)

	selected := 0
	if prev.ProviderID != "" {
		selected = ai.SelectorPickIndex(m.modelSelector.Flat, prev.ProviderID, prev.ID)
	}
	m.modelSelector.Selected = selected
	m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
	m.input.Placeholder = m.modelSelectorPlaceholderText()
	return m
}

func (m Model) modelSelectorPlaceholderText() string {
	if !m.modelSelectorActive() {
		return modelSelectorPlaceholder
	}
	for _, group := range m.modelSelector.Groups {
		if group.ProviderID == m.modelSelector.ProviderFilterID {
			return fmt.Sprintf("Filter %s models...", group.ProviderName)
		}
	}
	return modelSelectorPlaceholder
}

func (s ModelSelectorState) selectedModel() provider.ResolvedModel {
	if s.Selected < 0 || s.Selected >= len(s.Flat) {
		return provider.ResolvedModel{}
	}
	return s.Flat[s.Selected]
}

func (m Model) syncModelSelectorScroll(state ModelSelectorState) ModelSelectorState {
	if len(state.Flat) == 0 {
		state.Scroll = 0
		return state
	}
	if state.Selected < 0 {
		state.Selected = 0
	}
	if state.Selected >= len(state.Flat) {
		state.Selected = len(state.Flat) - 1
	}
	if state.Selected < state.Scroll {
		state.Scroll = state.Selected
	}
	if state.Selected >= state.Scroll+modelSelectorVisibleModels {
		state.Scroll = state.Selected - modelSelectorVisibleModels + 1
	}
	maxScroll := max(len(state.Flat)-modelSelectorVisibleModels, 0)
	if state.Scroll > maxScroll {
		state.Scroll = maxScroll
	}
	if state.Scroll < 0 {
		state.Scroll = 0
	}
	return state
}

func (m Model) handleModelSelectorKey(msg tea.KeyPressMsg) (Model, tea.Cmd, bool) {
	if !m.modelSelectorActive() {
		return m, nil, false
	}

	switch {
	case isInputEscapeKey(msg):
		m = m.closeModelSelector()
		m = m.syncLayout(m.content.AtBottom())
		return m, nil, true
	case msg.Code == tea.KeyEnter:
		return m.confirmModelSelector()
	case msg.Code == tea.KeyUp:
		m.modelSelector.Selected = max(m.modelSelector.Selected-1, 0)
		m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
		return m, nil, true
	case msg.Code == tea.KeyDown:
		if len(m.modelSelector.Flat) > 0 {
			m.modelSelector.Selected = min(m.modelSelector.Selected+1, len(m.modelSelector.Flat)-1)
			m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
		}
		return m, nil, true
	case msg.Code == tea.KeyLeft:
		// Only cycle provider when nothing to navigate in the filter.
		if len(m.input.Value()) == 0 {
			m = m.cycleModelSelectorProvider(-1)
			return m, nil, true
		}
	case msg.Code == tea.KeyRight:
		if len(m.input.Value()) == 0 {
			m = m.cycleModelSelectorProvider(1)
			return m, nil, true
		}
	}
	return m, nil, false
}

func (m Model) confirmModelSelector() (Model, tea.Cmd, bool) {
	if len(m.modelSelector.Flat) == 0 {
		m, cmd := m.withMessage("/model: no models match your search")
		m = m.closeModelSelector()
		m = m.syncLayout(true)
		return m, cmd, true
	}

	model := m.modelSelector.selectedModel()
	regProvider, ok := m.modelSelector.Catalog.Provider(model.ProviderID)
	if !ok {
		m, cmd := m.withMessage(fmt.Sprintf("/model: provider %q not found", model.ProviderID))
		m = m.closeModelSelector()
		m = m.syncLayout(true)
		return m, cmd, true
	}

	cfg, err := provider.BuildModelConfig(m.modelSelector.Catalog, regProvider, model)
	if err != nil && !provider.IsCredentialError(err) {
		m, cmd := m.withMessage(fmt.Sprintf("/model: %v", err))
		m = m.closeModelSelector()
		m = m.syncLayout(true)
		return m, cmd, true
	}

	m = m.applyModelSwitch(&command.ModelSwitch{
		Provider:      cfg.Provider,
		ProviderID:    cfg.ProviderID,
		ProviderName:  cfg.ProviderName,
		ModelID:       cfg.ModelID,
		ModelName:     cfg.ModelName,
		ContextWindow: cfg.ContextWindow,
		MaxTokens:     cfg.MaxTokens,
		Input:         model.Input,
		Cost:          model.Cost,
		Catalog:       m.modelSelector.Catalog,
	})
	var cmd tea.Cmd
	if err != nil {
		m, cmd = m.withMessage(fmt.Sprintf(
			"Selected %s [%s] — %s",
			cfg.ModelName,
			cfg.ProviderName,
			provider.CredentialHint(regProvider),
		))
	} else {
		m, cmd = m.withMessage(fmt.Sprintf("Switched to %s [%s]", cfg.ModelName, cfg.ProviderName))
	}
	if m.pendingPromptDraft != nil && isModelSlashInput(m.pendingPromptDraft.value) {
		m.pendingPromptDraft = nil
	}
	m = m.closeModelSelector()
	m = m.syncLayout(true)
	return m, cmd, true
}

func (m Model) applyModelSwitch(sw *command.ModelSwitch) Model {
	if sw == nil {
		return m
	}
	m.session.Provider = sw.Provider
	m.session.ProviderID = sw.ProviderID
	m.session.ProviderName = sw.ProviderName
	m.session.ModelID = sw.ModelID
	m.session.ModelName = sw.ModelName
	m.session.ContextWindow = sw.ContextWindow
	m.session.MaxTokens = sw.MaxTokens
	m.session.EnabledModelCount = sw.Catalog.TotalEnabledModels()
	m.session.Catalog = provider.TrimCatalogForRuntime(sw.Catalog, sw.ProviderID, sw.ModelID)
	m.modelName = sw.ModelName
	m.provider = sw.ProviderName
	m.contextWindow = sw.ContextWindow
	m.modelSupportsImage = provider.SupportsImageInput(sw.Input)
	m.modelCost = sw.Cost
	model, modelOK := sw.Catalog.Model(sw.ProviderID, sw.ModelID)
	if modelOK {
		m.thinkingLevel = provider.ClampThinkingLevel(m.thinkingLevel, model)
	}
	if m.contextWindow > 0 {
		m.contextUsed = min(float64(m.tokensUsed)/float64(m.contextWindow), 1.0)
	}
	if err := settings.SetActiveModel(sw.ProviderID, sw.ModelID); err != nil {
		m, _ = m.withMessage(fmt.Sprintf("Could not save model selection: %v", err))
		return m
	}
	if modelOK {
		_ = settings.SetThinkingLevel(m.thinkingLevel)
	}
	return m
}

// modelSelectorChromeView stacks the model list flush above the filter input,
// sharing a border seam like the slash-command palette.
func (m Model) modelSelectorChromeView() string {
	return lipgloss.JoinVertical(lipgloss.Top, m.modelSelectorListBox(), m.modelSelectorFilterBox())
}

func (m Model) modelSelectorListBox() string {
	boxW := borderedChromeWidth(m.chromeOuterWidth())
	return modelSelectorListBorder(m.mode).Width(boxW).Render(m.modelSelectorListBody())
}

func (m Model) modelSelectorFilterBox() string {
	return m.inputBoxView(true)
}

func (m Model) modelSelectorView() string {
	if !m.modelSelectorActive() {
		return ""
	}
	return m.modelSelectorChromeView()
}

func (m Model) modelSelectorListBody() string {
	flat := m.modelSelector.Flat
	if len(flat) == 0 {
		return dimStyle.Render("No matching models")
	}

	showProvider := m.modelSelectorShowProviderPerRow()
	lines := make([]string, 0, len(flat)+2)
	if header := m.modelSelectorProviderHeader(); header != "" {
		lines = append(lines, header)
	}

	names := make([]string, len(flat))
	summaries := make([]string, len(flat))
	for i, model := range flat {
		names[i] = modelDisplayName(model)
		summaries[i] = modelSelectorSummaryPlain(model, m.session.ProviderID, m.session.ModelID, showProvider)
	}
	nameColW := align.ColumnWidth(names...)

	end := min(m.modelSelector.Scroll+modelSelectorVisibleModels, len(flat))
	for i := m.modelSelector.Scroll; i < end; i++ {
		model := flat[i]
		selected := i == m.modelSelector.Selected

		var nameStyled string
		if selected {
			nameStyled = cmdPaletteSelected.Render(names[i])
		} else {
			nameStyled = cmdPaletteName.Render(names[i])
		}

		_, gap, _ := align.Row(names[i], nameColW, summaries[i])
		summaryStyled := modelSelectorSummaryStyled(model, m.session.ProviderID, m.session.ModelID, selected, showProvider)
		lines = append(lines, modelSelectorLine(nameStyled, gap, summaryStyled))
	}
	if end < len(flat) {
		lines = append(lines, dimStyle.Render(fmt.Sprintf("%d more ↓", len(flat)-end)))
	}
	return strings.Join(lines, "\n")
}

func (m Model) modelSelectorShowProviderPerRow() bool {
	if len(m.modelSelector.Groups) <= 1 {
		return false
	}
	return m.modelSelector.ProviderFilterID == ""
}

func (m Model) modelSelectorProviderHeader() string {
	switch {
	case len(m.modelSelector.Groups) == 0:
		return ""
	case len(m.modelSelector.Groups) == 1:
		return dimStyle.Render(m.modelSelector.Groups[0].ProviderName)
	case m.modelSelector.ProviderFilterID == "":
		return dimStyle.Render("All providers  ← →")
	}
	for _, group := range m.modelSelector.Groups {
		if group.ProviderID == m.modelSelector.ProviderFilterID {
			return dimStyle.Render(group.ProviderName + "  ← →")
		}
	}
	return ""
}

func modelDisplayName(model provider.ResolvedModel) string {
	if model.Name != "" && model.Name != model.ID {
		return model.Name
	}
	return model.ID
}

func modelSelectorLine(name, gap, summaryStyled string) string {
	return name + gap + summaryStyled
}

func modelProviderName(model provider.ResolvedModel) string {
	if model.ProviderName != "" {
		return model.ProviderName
	}
	return model.ProviderID
}

func modelSelectorSummaryPlain(model provider.ResolvedModel, activeProviderID, activeModelID string, showProvider bool) string {
	isCurrent := model.ProviderID == activeProviderID && model.ID == activeModelID
	var parts []string
	if showProvider {
		parts = append(parts, modelProviderName(model))
	}
	if isCurrent {
		parts = append(parts, modelSelectorCurrentMark+modelSelectorCurrentLabel)
	}
	return strings.Join(parts, "  ")
}

func modelSelectorSummaryStyled(model provider.ResolvedModel, activeProviderID, activeModelID string, selected bool, showProvider bool) string {
	isCurrent := model.ProviderID == activeProviderID && model.ID == activeModelID

	var styled []string
	if showProvider {
		providerName := modelProviderName(model)
		if selected {
			styled = append(styled, cmdPaletteSummarySelected.Render(providerName))
		} else {
			styled = append(styled, dimStyle.Render(providerName))
		}
	}
	if isCurrent {
		styled = append(styled, modelSelectorCurrentMarker.Render(modelSelectorCurrentMark+modelSelectorCurrentLabel))
	}
	return strings.Join(styled, "  ")
}

func (m Model) modelSelectorListHeight() int {
	if !m.modelSelectorActive() {
		return 0
	}
	return lipgloss.Height(m.modelSelectorListBox())
}
