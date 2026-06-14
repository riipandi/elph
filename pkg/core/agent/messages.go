package agent

import (
	"strings"

	"github.com/riipandi/elph/pkg/ai/provider"
)

// prepareTurnMessages merges retained history with the current user prompt.
// When history is non-empty, opts.UserPrompt must still be appended or the model
// only sees prior turns and re-answers the first question.
func prepareTurnMessages(opts TurnOptions) []provider.ChatMessage {
	messages := CompactMessages(append([]provider.ChatMessage(nil), opts.Messages...))
	prompt := strings.TrimSpace(opts.UserPrompt)
	images := opts.UserImages
	if len(images) > 0 {
		images = append([]provider.ImageAttachment(nil), images...)
	}
	if prompt == "" && len(images) == 0 {
		return messages
	}
	if len(messages) > 0 {
		last := messages[len(messages)-1]
		if last.Role == "user" && strings.TrimSpace(last.Content) == prompt && len(images) == 0 && len(last.Images) == 0 {
			return messages
		}
	}
	return append(messages, provider.ChatMessage{Role: "user", Content: prompt, Images: images})
}
