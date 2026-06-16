package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func main() {
	s := mcp.NewServer(&mcp.Implementation{Name: "elph-tools", Version: "0.1.0"}, nil)
	registerTools(s)
	log.Println("elph-tools MCP server ready (stdio)")
	if err := s.Run(context.Background(), &mcp.StdioTransport{}); err != nil {
		log.Fatal(err)
	}
}

// ── Input types (jsonschema tags drive auto schema) ──────────────────

type ReadInput struct {
	Path       string `json:"path" jsonschema:"required,file path"`
	LineOffset int    `json:"line_offset,omitempty" jsonschema:"starting line (1-indexed, negative=tail)"`
	Nlines     int    `json:"n_lines,omitempty" jsonschema:"max lines to read"`
}

type WriteInput struct {
	Path    string `json:"path" jsonschema:"required,file path"`
	Content string `json:"content" jsonschema:"required,content to write"`
	Mode    string `json:"mode,omitempty" jsonschema:"overwrite (default) or append"`
}

type EditInput struct {
	Path       string `json:"path" jsonschema:"required,file path"`
	OldString  string `json:"old_string" jsonschema:"required,exact text to replace"`
	NewString  string `json:"new_string" jsonschema:"required,replacement text"`
	ReplaceAll bool   `json:"replace_all,omitempty" jsonschema:"replace every occurrence"`
}

type GrepInput struct {
	Pattern      string `json:"pattern" jsonschema:"required,regular expression"`
	Path         string `json:"path,omitempty" jsonschema:"search directory (default .)"`
	Glob         string `json:"glob,omitempty" jsonschema:"glob filter"`
	OutputMode   string `json:"output_mode,omitempty" jsonschema:"content, files_with_matches, count"`
	ContextLines int    `json:"context_lines,omitempty" jsonschema:"context lines to show"`
}

type GlobInput struct {
	Pattern string `json:"pattern" jsonschema:"required,glob pattern"`
	Path    string `json:"path,omitempty" jsonschema:"search directory"`
}

type BashInput struct {
	Command string `json:"command" jsonschema:"required,shell command"`
	Cwd     string `json:"cwd,omitempty" jsonschema:"working directory"`
	Timeout int    `json:"timeout,omitempty" jsonschema:"timeout in seconds (max 300)"`
}

type WebSearchInput struct {
	Query  string `json:"query" jsonschema:"required,search query"`
	Engine string `json:"engine,omitempty" jsonschema:"duckduckgo, jina, brave, tavily"`
	Limit  int    `json:"limit,omitempty" jsonschema:"max results (1-20, default 5)"`
}

type FetchURLInput struct {
	URL string `json:"url" jsonschema:"required,URL to fetch"`
}

// ── Register tools ───────────────────────────────────────────────────

func registerTools(s *mcp.Server) {
	mcp.AddTool(s, &mcp.Tool{Name: "Read", Description: "Read a text file with line numbers. Supports line_offset (negative=tails) and n_lines."},
		func(ctx context.Context, ss *mcp.ServerSession, in *mcp.CallToolParamsFor[ReadInput]) (*mcp.CallToolResultFor[struct{}], error) {
			return handleRead(in.Arguments), nil
		})

	mcp.AddTool(s, &mcp.Tool{Name: "Write", Description: "Create, overwrite, or append to a file. Parent directories created."},
		func(ctx context.Context, ss *mcp.ServerSession, in *mcp.CallToolParamsFor[WriteInput]) (*mcp.CallToolResultFor[struct{}], error) {
			return handleWrite(in.Arguments), nil
		})

	mcp.AddTool(s, &mcp.Tool{Name: "Edit", Description: "Replace exact text in a file. Supports replace_all."},
		func(ctx context.Context, ss *mcp.ServerSession, in *mcp.CallToolParamsFor[EditInput]) (*mcp.CallToolResultFor[struct{}], error) {
			return handleEdit(in.Arguments), nil
		})

	mcp.AddTool(s, &mcp.Tool{Name: "Grep", Description: "Full-text search with ripgrep. Supports context_lines, glob, output_mode."},
		func(ctx context.Context, ss *mcp.ServerSession, in *mcp.CallToolParamsFor[GrepInput]) (*mcp.CallToolResultFor[struct{}], error) {
			return handleGrep(ctx, in.Arguments), nil
		})

	mcp.AddTool(s, &mcp.Tool{Name: "Glob", Description: "Find files by glob pattern."},
		func(ctx context.Context, ss *mcp.ServerSession, in *mcp.CallToolParamsFor[GlobInput]) (*mcp.CallToolResultFor[struct{}], error) {
			return handleGlob(in.Arguments), nil
		})

	mcp.AddTool(s, &mcp.Tool{Name: "Bash", Description: "Execute a shell command. Supports cwd and timeout (default 120s, max 300s)."},
		func(ctx context.Context, ss *mcp.ServerSession, in *mcp.CallToolParamsFor[BashInput]) (*mcp.CallToolResultFor[struct{}], error) {
			return handleBash(ctx, in.Arguments), nil
		})

	mcp.AddTool(s, &mcp.Tool{Name: "WebSearch", Description: "Search the web (DuckDuckGo, Jina, Brave, Tavily)."},
		func(ctx context.Context, ss *mcp.ServerSession, in *mcp.CallToolParamsFor[WebSearchInput]) (*mcp.CallToolResultFor[struct{}], error) {
			return handleWebSearch(ctx, in.Arguments), nil
		})

	mcp.AddTool(s, &mcp.Tool{Name: "FetchURL", Description: "Fetch a URL as plain text."},
		func(ctx context.Context, ss *mcp.ServerSession, in *mcp.CallToolParamsFor[FetchURLInput]) (*mcp.CallToolResultFor[struct{}], error) {
			return handleFetchURL(ctx, in.Arguments), nil
		})
}

func textRes(text string) *mcp.CallToolResultFor[struct{}] {
	return &mcp.CallToolResultFor[struct{}]{
		Content: []mcp.Content{&mcp.TextContent{Text: text}},
	}
}

func errRes(text string) *mcp.CallToolResultFor[struct{}] {
	return &mcp.CallToolResultFor[struct{}]{
		IsError: true,
		Content: []mcp.Content{&mcp.TextContent{Text: text}},
	}
}


// ── Tool handlers ────────────────────────────────────────────────────

func handleRead(in ReadInput) *mcp.CallToolResultFor[struct{}] {
	full := resolvePath(in.Path)
	data, err := os.ReadFile(full)
	if err != nil {
		return errRes(fmt.Sprintf("cannot read %s: %v", in.Path, err))
	}
	lines := strings.Split(string(data), "\n")
	total := len(lines)

	offset := in.LineOffset
	if offset == 0 {
		offset = 1
	}
	if offset < 0 {
		offset = total + offset + 1
	}
	if offset < 1 {
		offset = 1
	}
	if offset > total {
		return textRes("(end of file)")
	}

	start := offset - 1
	end := total
	if in.Nlines > 0 && start+in.Nlines < total {
		end = start + in.Nlines
	}

	var b bytes.Buffer
	for i := start; i < end; i++ {
		b.WriteString(fmt.Sprintf("%d\t%s\n", i+1, lines[i]))
	}
	b.WriteString(fmt.Sprintf("<system>Total: %d lines. Shown: %d-%d.</system>", total, offset, end))
	return textRes(b.String())
}

func handleWrite(in WriteInput) *mcp.CallToolResultFor[struct{}] {
	full := resolvePath(in.Path)
	if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
		return errRes(fmt.Sprintf("cannot create dir: %v", err))
	}
	if strings.EqualFold(in.Mode, "append") {
		f, err := os.OpenFile(full, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
		if err != nil {
			return errRes(fmt.Sprintf("cannot open: %v", err))
		}
		defer f.Close()
		if _, err := f.WriteString(in.Content); err != nil {
			return errRes(fmt.Sprintf("append failed: %v", err))
		}
		return textRes(fmt.Sprintf("Appended %d bytes", len(in.Content)))
	}
	if err := os.WriteFile(full, []byte(in.Content), 0o644); err != nil {
		return errRes(fmt.Sprintf("write failed: %v", err))
	}
	return textRes(fmt.Sprintf("Wrote %d bytes", len(in.Content)))
}

func handleEdit(in EditInput) *mcp.CallToolResultFor[struct{}] {
	if in.OldString == in.NewString {
		return errRes("old_string and new_string are identical")
	}
	full := resolvePath(in.Path)
	data, err := os.ReadFile(full)
	if err != nil {
		return errRes(fmt.Sprintf("cannot read %s: %v", in.Path, err))
	}
	content := string(data)
	if !strings.Contains(content, in.OldString) {
		return errRes(fmt.Sprintf("old_string not found in %s", in.Path))
	}
	count := strings.Count(content, in.OldString)
	if count > 1 && !in.ReplaceAll {
		return errRes(fmt.Sprintf("old_string appears %d times; set replace_all", count))
	}
	if in.ReplaceAll {
		content = strings.ReplaceAll(content, in.OldString, in.NewString)
	} else {
		content = strings.Replace(content, in.OldString, in.NewString, 1)
	}
	if err := os.WriteFile(full, []byte(content), 0o644); err != nil {
		return errRes(fmt.Sprintf("write failed: %v", err))
	}
	return textRes(fmt.Sprintf("Replaced in %s", in.Path))
}

func handleGrep(ctx context.Context, in GrepInput) *mcp.CallToolResultFor[struct{}] {
	if _, err := exec.LookPath("rg"); err != nil {
		return errRes("ripgrep not found in PATH")
	}
	searchPath := in.Path
	if searchPath == "" {
		searchPath = "."
	}
	args := []string{"--regexp", in.Pattern, "--color=never"}
	if in.ContextLines > 0 {
		args = append(args, "-C", fmt.Sprint(in.ContextLines))
	}
	switch in.OutputMode {
	case "files_with_matches":
		args = append(args, "--files-with-matches")
	case "count":
		args = append(args, "--count")
	default:
		args = append(args, "--line-number", "--with-filename")
	}
	if in.Glob != "" {
		args = append(args, "--glob", in.Glob)
	}
	args = append(args, searchPath)
	out, err := exec.CommandContext(ctx, "rg", args...).CombinedOutput()
	if err != nil {
		if exit, ok := err.(*exec.ExitError); ok && exit.ExitCode() == 1 {
			return textRes("(no matches)")
		}
		return errRes(fmt.Sprintf("grep failed: %v", err))
	}
	return textRes(strings.TrimRight(string(out), "\n"))
}

func handleGlob(in GlobInput) *mcp.CallToolResultFor[struct{}] {
	root := "."
	if in.Path != "" {
		root = in.Path
	}
	search := in.Pattern
	if !filepath.IsAbs(search) {
		search = filepath.Join(root, search)
	}
	matches, err := filepath.Glob(search)
	if err != nil {
		return errRes(fmt.Sprintf("glob error: %v", err))
	}
	sort.Strings(matches)
	if len(matches) > 500 {
		matches = matches[:500]
	}
	if len(matches) == 0 {
		return textRes("(no matches)")
	}
	return textRes(strings.Join(matches, "\n"))
}

func handleBash(ctx context.Context, in BashInput) *mcp.CallToolResultFor[struct{}] {
	cmd := strings.TrimSpace(in.Command)
	if cmd == "" || strings.Contains(cmd, "\x00") {
		return errRes("invalid command")
	}
	wd := "."
	if in.Cwd != "" {
		wd = in.Cwd
	}
	timeout := 120
	if in.Timeout > 0 {
		timeout = min(in.Timeout, 300)
	}
	ctx, cancel := context.WithTimeout(ctx, time.Duration(timeout)*time.Second)
	defer cancel()

	execCmd := exec.CommandContext(ctx, "bash", "-c", cmd)
	execCmd.Dir = wd
	execCmd.Env = append(os.Environ(), "NO_COLOR=1", "TERM=dumb")
	out, err := execCmd.CombinedOutput()
	if err != nil {
		if ctx.Err() != nil {
			return errRes(fmt.Sprintf("timed out (%ds)", timeout))
		}
		o := strings.TrimSpace(string(out))
		if o == "" {
			return errRes(fmt.Sprintf("exit: %d", execCmd.ProcessState.ExitCode()))
		}
		return textRes(fmt.Sprintf("%s\n\n(exit %d)", o, execCmd.ProcessState.ExitCode()))
	}
	return textRes(strings.TrimRight(string(out), "\n"))
}

// ── Web tools ────────────────────────────────────────────────────────

type searchResult struct {
	Title   string
	URL     string
	Snippet string
}

func handleWebSearch(ctx context.Context, in WebSearchInput) *mcp.CallToolResultFor[struct{}] {
	engine := strings.ToLower(in.Engine)
	if engine == "" {
		engine = "duckduckgo"
	}
	limit := in.Limit
	if limit < 1 || limit > 20 {
		limit = 5
	}
	var results []searchResult
	var err error
	switch engine {
	case "duckduckgo", "ddg":
		results, err = searchDDG(ctx, in.Query, limit)
	case "jina":
		results, err = searchJina(ctx, in.Query, limit)
	case "brave":
		results, err = searchBrave(ctx, in.Query, limit)
	case "tavily":
		results, err = searchTavily(ctx, in.Query, limit)
	default:
		results, err = searchDDG(ctx, in.Query, limit)
	}
	if err != nil {
		return errRes(fmt.Sprintf("search failed: %v", err))
	}
	var b bytes.Buffer
	fmt.Fprintf(&b, "engine: %s\nquery: %s\nresults: %d\n", engine, in.Query, len(results))
	for i, r := range results {
		b.WriteString(fmt.Sprintf("\n%d. %s\n   url: %s\n   snippet: %s", i+1, r.Title, r.URL, r.Snippet))
	}
	return textRes(b.String())
}

func handleFetchURL(ctx context.Context, in FetchURLInput) *mcp.CallToolResultFor[struct{}] {
	client := &http.Client{Timeout: 30 * time.Second}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, in.URL, nil)
	if err != nil {
		return errRes(fmt.Sprintf("invalid URL: %v", err))
	}
	req.Header.Set("User-Agent", "Elph-MCP/0.1.0")
	resp, err := client.Do(req)
	if err != nil {
		return errRes(fmt.Sprintf("fetch failed: %v", err))
	}
	defer resp.Body.Close()

	const maxBytes = 256 << 10
	body, err := io.ReadAll(io.LimitReader(resp.Body, maxBytes+1))
	if err != nil {
		return errRes(fmt.Sprintf("read failed: %v", err))
	}
	truncated := len(body) > maxBytes
	if truncated {
		body = body[:maxBytes]
	}
	ct := resp.Header.Get("Content-Type")
	content := string(body)
	if strings.Contains(strings.ToLower(ct), "text/html") || strings.Contains(strings.ToLower(ct), "application/xhtml") {
		content = stripHTML(body)
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		msg := content
		if len(msg) > 240 {
			msg = msg[:240] + "..."
		}
		return errRes(fmt.Sprintf("HTTP %d: %s", resp.StatusCode, msg))
	}
	if truncated {
		content += "\n\n(output truncated)"
	}
	var b bytes.Buffer
	b.WriteString(fmt.Sprintf("URL: %s\n", resp.Request.URL.String()))
	if ct != "" {
		b.WriteString(fmt.Sprintf("Content-Type: %s\n", ct))
	}
	b.WriteString("\n" + strings.TrimRight(content, "\n"))
	return textRes(b.String())
}

// ── Search engine impls ──────────────────────────────────────────────

func searchDDG(ctx context.Context, query string, limit int) ([]searchResult, error) {
	u := fmt.Sprintf("https://html.duckduckgo.com/html/?q=%s", url.QueryEscape(query))
	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	req.Header.Set("User-Agent", "Mozilla/5.0 (compatible; Elph-MCP)")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	html := string(body)

	var results []searchResult
	parts := strings.Split(html, `<a rel="nofollow" class="result__a" href="`)
	for i := 1; i < len(parts) && len(results) < limit; i++ {
		part := parts[i]
		rawURL := extractUntil(part, `"`)
		title := stripTags(extractBetween(part, `>`, `</a>`))
		snippet := stripTags(extractBetween(part, `<a class="result__snippet">`, `</a>`))
		if title != "" && rawURL != "" {
			decodedURL, _ := url.QueryUnescape(rawURL)
			results = append(results, searchResult{
				Title: decodeEntities(title), URL: decodeEntities(decodedURL),
				Snippet: decodeEntities(snippet),
			})
		}
	}
	if len(results) == 0 {
		return nil, fmt.Errorf("no results")
	}
	return results, nil
}

func searchJina(ctx context.Context, query string, limit int) ([]searchResult, error) {
	apiKey := os.Getenv("JINA_API_KEY")
	if apiKey == "" {
		return nil, fmt.Errorf("JINA_API_KEY not set")
	}
	u := fmt.Sprintf("https://s.jina.ai/%s", url.QueryEscape(query))
	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	req.Header.Set("Authorization", "Bearer "+apiKey)
	req.Header.Set("Accept", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var data struct {
		Data []struct {
			Title   string `json:"title"`
			URL     string `json:"url"`
			Content string `json:"content"`
		} `json:"data"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}
	var results []searchResult
	for _, item := range data.Data {
		if len(results) >= limit {
			break
		}
		snippet := item.Content
		if len(snippet) > 300 {
			snippet = snippet[:300] + "..."
		}
		results = append(results, searchResult{Title: item.Title, URL: item.URL, Snippet: snippet})
	}
	return results, nil
}

func searchBrave(ctx context.Context, query string, limit int) ([]searchResult, error) {
	apiKey := os.Getenv("BRAVE_SEARCH_API_KEY")
	if apiKey == "" {
		return nil, fmt.Errorf("BRAVE_SEARCH_API_KEY not set")
	}
	u := fmt.Sprintf("https://api.search.brave.com/res/v1/web/search?q=%s&count=%d", url.QueryEscape(query), limit)
	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	req.Header.Set("X-Subscription-Token", apiKey)
	req.Header.Set("Accept", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var data struct {
		Web struct {
			Results []struct {
				Title       string `json:"title"`
				URL         string `json:"url"`
				Description string `json:"description"`
			} `json:"results"`
		} `json:"web"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}
	var results []searchResult
	for _, item := range data.Web.Results {
		if len(results) >= limit {
			break
		}
		results = append(results, searchResult{Title: item.Title, URL: item.URL, Snippet: item.Description})
	}
	return results, nil
}

func searchTavily(ctx context.Context, query string, limit int) ([]searchResult, error) {
	apiKey := os.Getenv("TAVILY_API_KEY")
	if apiKey == "" {
		return nil, fmt.Errorf("TAVILY_API_KEY not set")
	}
	payload := map[string]any{"api_key": apiKey, "query": query, "max_results": limit}
	body, _ := json.Marshal(payload)
	req, _ := http.NewRequestWithContext(ctx, http.MethodPost, "https://api.tavily.com/search", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var data struct {
		Results []struct {
			Title   string `json:"title"`
			URL     string `json:"url"`
			Content string `json:"content"`
		} `json:"results"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}
	var results []searchResult
	for _, item := range data.Results {
		if len(results) >= limit {
			break
		}
		snippet := item.Content
		if len(snippet) > 300 {
			snippet = snippet[:300] + "..."
		}
		results = append(results, searchResult{Title: item.Title, URL: item.URL, Snippet: snippet})
	}
	return results, nil
}

// ── Helpers ──────────────────────────────────────────────────────────

func resolvePath(p string) string {
	if filepath.IsAbs(p) {
		return filepath.Clean(p)
	}
	wd, _ := os.Getwd()
	return filepath.Clean(filepath.Join(wd, p))
}

func stripHTML(data []byte) string {
	var b bytes.Buffer
	in := false
	for _, r := range string(data) {
		switch {
		case r == '<':
			in = true
		case r == '>':
			in = false
		case !in:
			b.WriteRune(r)
		}
	}
	return strings.TrimSpace(b.String())
}

func extractBetween(s, start, end string) string {
	i := strings.Index(s, start)
	if i < 0 {
		return ""
	}
	s = s[i+len(start):]
	j := strings.Index(s, end)
	if j < 0 {
		return ""
	}
	return s[:j]
}

func extractUntil(s, delim string) string {
	i := strings.Index(s, delim)
	if i < 0 {
		return s
	}
	return s[:i]
}

func stripTags(s string) string {
	var b bytes.Buffer
	in := false
	for _, r := range s {
		switch {
		case r == '<':
			in = true
		case r == '>':
			in = false
		case !in:
			b.WriteRune(r)
		}
	}
	return strings.TrimSpace(b.String())
}

func decodeEntities(s string) string {
	s = strings.ReplaceAll(s, "&amp;", "&")
	s = strings.ReplaceAll(s, "&lt;", "<")
	s = strings.ReplaceAll(s, "&gt;", ">")
	s = strings.ReplaceAll(s, "&quot;", "\"")
	s = strings.ReplaceAll(s, "&#39;", "'")
	s = strings.ReplaceAll(s, "&nbsp;", " ")
	return s
}
