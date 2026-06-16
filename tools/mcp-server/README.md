# elph-tools MCP Server

A Go-based MCP (Model Context Protocol) server providing coding agent tools:
file I/O, shell commands, web search, and URL fetching.

## Tools

| Tool       | Description                                         |
|------------|-----------------------------------------------------|
| Read       | Read text files with line_offset and n_lines        |
| Write      | Create, overwrite, or append files                  |
| Edit       | String replacement in files (replace_all support)   |
| Grep       | Full-text search via ripgrep                        |
| Glob       | Find files by glob pattern                          |
| Bash       | Execute shell commands (cwd, timeout support)       |
| WebSearch  | Web search (DuckDuckGo, Jina, Brave, Tavily)        |
| FetchURL   | Fetch URL content as plain text                     |

## Usage

Build:

```bash
go build -o mcp-server .
```

Configure in your MCP client (e.g. Claude Desktop, Cursor, or any MCP host):

```json
{
  "mcpServers": {
    "elph-tools": {
      "command": "/path/to/mcp-server",
      "args": []
    }
  }
}
```

## Configuration

Search engines require API keys via environment variables:

| Engine     | Env Variable           | Required |
|------------|------------------------|----------|
| DuckDuckGo | — (free, no key)      | No       |
| Jina       | `JINA_API_KEY`         | Yes      |
| Brave      | `BRAVE_SEARCH_API_KEY` | Yes      |
| Tavily     | `TAVILY_API_KEY`       | Yes      |
