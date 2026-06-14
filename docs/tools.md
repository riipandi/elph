# Built-in Tools

## Permissions

| Permission          | Description                                              |
|---------------------|----------------------------------------------------------|
| `auto-allow`        | Runs without approval by default; user can require it    |
| `requires-approval` | Requires user approval before each run                   |
| `always-approve`    | Always runs without approval; cannot be restricted       |

## File Tools

File tools handle reading, writing, and searching the local filesystem - the foundation for code analysis
and modification tasks.

| Tool          | Default Approval  | Description                        |
|---------------|-------------------|------------------------------------|
| Read          | Auto-allow        | Read a text file's contents        |
| Write         | Requires approval | Create or overwrite a file         |
| Edit          | Requires approval | Precise string replacement         |
| Grep          | Auto-allow        | `ripgrep` powered full-text search |
| Glob          | Auto-allow        | Find files by glob pattern         |
| ReadMediaFile | Auto-allow        | Read an image or video file        |

## Shell Tools

| Tool | Default Approval  | Description             |
|------|-------------------|-------------------------|
| Bash | Requires approval | Execute a shell command |

## Web Tools

| Tool       | Default Approval | Description                          |
|------------|------------------|--------------------------------------|
| FetchURL   | Auto-allow       | Fetch the content of a specified URL |
| WebSearch  | Auto-allow       | Web search with multiple engines     |
| CodeSearch | Auto-allow       | Search code on GitHub                |

## Plan Mode

| Tool          | Default Approval | Description                        |
|---------------|------------------|------------------------------------|
| EnterPlanMode | Auto-allow       | Enter Plan mode                    |
| ExitPlanMode  | Auto-allow       | Exit Plan mode and submit the plan |

`ExitPlanMode` will requires user to confirm the plan.

## Collaboration Tools

Collaboration tools handle inter-Agent coordination, user interaction, and Skill invocation.

| Tool    | Default Approval | Description                                        |
|---------|------------------|----------------------------------------------------|
| AskUser | Auto-allow       | Ask the user a question to gather structured input |
