package providertests

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/prompt"
	"github.com/riipandi/elph/pkg/ai"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/tool"
)

// Live probe for opencode big-pickle hangs. Run manually:
//
//	OPENCODE_API_KEY=... go test ./pkg/ai/providertests -run TestLiveBigPickleProbe -count=1 -v -timeout 3m
func TestLiveBigPickleProbe(t *testing.T) {
	if testing.Short() {
		t.Skip("live probe skipped in -short mode")
	}
	if os.Getenv("OPENCODE_API_KEY") == "" {
		t.Skip("OPENCODE_API_KEY not set")
	}

	cfg := ai.ResolveProvider("opencode", "big-pickle")
	if cfg.Provider == nil {
		t.Fatal("provider not resolved")
	}
	model, ok := cfg.Catalog.Model("opencode", "big-pickle")
	if !ok {
		t.Fatal("model not in catalog")
	}

	userPrompt := "你是谁？你是什么模型？谁创造了你？你的知识库何时过期？你的上下文长度限制是多少？请用印尼语回答，只需给出答案，不要提问."
	systemPrompt := prompt.Build(prompt.Options{WorkDir: mustRepoRoot(t)})
	thinking := provider.ResolveThinking(model, constants.ThinkingHigh, nil)
	compat := model.Compat
	if compat.ThinkingFormat == "" {
		if reg, ok := cfg.Catalog.Provider("opencode"); ok {
			compat = reg.Config.Compat
		}
	}
	tools := tool.ProviderDefinitions()
	t.Logf("tools=%d thinking=%s enable_thinking=%v", len(tools), thinking.ThinkingFormat, thinking.EnableThinking)

	cases := []struct {
		name     string
		tools    []provider.ToolDefinition
		thinking provider.ThinkingConfig
		system   string
	}{
		{name: "minimal", tools: nil, thinking: provider.ThinkingConfig{}, system: ""},
		{name: "no_tools_thinking", tools: nil, thinking: thinking, system: systemPrompt},
		{name: "no_thinking_tools", tools: tools, thinking: provider.ThinkingConfig{}, system: systemPrompt},
		{name: "full", tools: tools, thinking: thinking, system: systemPrompt},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
			defer cancel()

			var (
				thinkingChunks int
				contentChunks  int
				firstEvent     time.Duration
				start          = time.Now()
			)
			stream := &provider.TurnStream{
				OnThinking: func(chunk string) {
					if thinkingChunks == 0 {
						firstEvent = time.Since(start)
					}
					thinkingChunks++
				},
				OnContent: func(chunk string) {
					if thinkingChunks+contentChunks == 0 {
						firstEvent = time.Since(start)
					}
					contentChunks++
				},
			}

			result, err := cfg.Provider.Complete(ctx, provider.TurnRequest{
				SystemPrompt: tc.system,
				UserPrompt:   userPrompt,
				Model:        "big-pickle",
				Thinking:     tc.thinking,
				Compat:       compat,
				Stream:       stream,
				Tools:        tc.tools,
				Messages: []provider.ChatMessage{
					{Role: "user", Content: userPrompt},
				},
			})

			elapsed := time.Since(start)
			if err != nil {
				msg := err.Error()
				if strings.Contains(msg, "Rate limit") || strings.Contains(msg, "FreeUsageLimitError") {
					t.Skipf("opencode rate limited after %s: %v", elapsed.Round(time.Millisecond), err)
				}
				t.Fatalf("after %s: %v (thinking_chunks=%d content_chunks=%d first=%s)",
					elapsed.Round(time.Millisecond), err, thinkingChunks, contentChunks, firstEvent)
			}
			t.Logf("ok in %s first=%s thinking_chunks=%d content_chunks=%d thinking_len=%d content_len=%d tool_calls=%d",
				elapsed.Round(time.Millisecond), firstEvent,
				thinkingChunks, contentChunks,
				len(result.Thinking), len(result.Content), len(result.ToolCalls))
			if thinkingChunks == 0 && contentChunks == 0 && strings.TrimSpace(result.Content) == "" && len(result.ToolCalls) == 0 {
				t.Fatal("empty stream and empty result")
			}
		})
	}
}

func mustRepoRoot(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatal("go.mod not found")
		}
		dir = parent
	}
}