package openai

import (
	"testing"

	provider "github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/stretchr/testify/require"
)

func TestUserMessageParamWithImage(t *testing.T) {
	t.Parallel()

	param := userMessageParam(provider.ChatMessage{
		Role:    "user",
		Content: "what is this?",
		Images: []provider.ImageAttachment{
			{MIME: "image/png", Data: []byte{0x89, 0x50, 0x4e, 0x47}},
		},
	})
	require.NotNil(t, param.OfUser)
	require.Len(t, param.OfUser.Content.OfArrayOfContentParts, 2)
}
