package renderer

import (
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
)

func (m Model) addProviderErrorDetail(err error) Model {
	if err == nil {
		return m
	}
	body := provider.FormatProviderErrorDetail(err)
	body = agent.TruncateWithNotice(body, agent.MaxDisplayToolBytes)
	m = m.addDetailMessageWithStatus("Provider error", body, constants.DetailStatusError)
	m.session.AppendLog("provider_error", body)
	return m
}
