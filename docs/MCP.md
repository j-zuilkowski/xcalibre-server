# MCP Server Integration

xcalibre-server exposes your library as an MCP (Model Context Protocol) tool provider. Any MCP-compatible agent can search your library, read book metadata, extract chapter text, and run semantic search as native tool calls.

## Available Tools

| Tool | Description | LLM Required |
|---|---|---|
| `search_books` | Full-text + filtered library search | No |
| `get_book_metadata` | Complete metadata for one book | No |
| `list_chapters` | Table of contents from EPUB or PDF | No |
| `get_book_text` | Plain text extraction, full or by chapter | No |
| `semantic_search` | Vector similarity search | Yes (`llm.enabled = true`) |

## Setup: Generate an API Token

Before connecting any client, generate a long-lived API token:

```bash
curl -X POST https://your-library/api/v1/admin/tokens \
  -H "Authorization: Bearer <your-jwt>" \
  -H "Content-Type: application/json" \
  -d '{"name": "claude-desktop"}'
```

Save the returned `token` value. It is shown only once and is never stored in plain text.

## Run the MCP Server

The `calibre-mcp` binary reads `config.toml` by default from the current working directory. If you keep the config elsewhere, set `CONFIG_PATH` before launching the server.

```bash
CONFIG_PATH=/path/to/config.toml calibre-mcp --transport stdio
CONFIG_PATH=/path/to/config.toml calibre-mcp --transport sse --port 8084
```

## Claude Code Integration

Add the server to Claude Code:

```bash
claude mcp add xcalibre-server-library ./target/release/calibre-mcp \
  --env CONFIG_PATH=./config.toml
```

Or add it manually to `.claude/settings.json`:

```json
{
  "mcpServers": {
    "xcalibre-server-library": {
      "command": "/path/to/calibre-mcp",
      "args": ["--transport", "stdio"],
      "env": {
        "CONFIG_PATH": "./config.toml"
      }
    }
  }
}
```

## Claude Desktop Integration

Add this to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "xcalibre-server-library": {
      "command": "/path/to/calibre-mcp",
      "args": ["--transport", "stdio"],
      "env": {
        "CONFIG_PATH": "./config.toml"
      }
    }
  }
}
```

## LangGraph / HTTP Agent Integration

Start the SSE transport:

```bash
CONFIG_PATH=/path/to/config.toml calibre-mcp --transport sse --port 8084
```

Connect from LangGraph:

```python
from langchain_mcp_adapters.client import MultiServerMCPClient

client = MultiServerMCPClient({
    "calibre": {
        "url": "http://localhost:8084/mcp/sse",
        "transport": "sse",
        "headers": {"Authorization": "Bearer <api-token>"}
    }
})

tools = await client.get_tools()
```

The SSE transport requires a bearer API token in the `Authorization` header. The token is hashed with SHA256 on the server and matched against stored API token hashes.

## smolagents Integration

```python
from smolagents import ToolCollection, CodeAgent, HfApiModel

tools = ToolCollection.from_mcp(
    {
        "url": "http://localhost:8084/mcp/sse",
        "headers": {"Authorization": "Bearer <api-token>"}
    }
)

agent = CodeAgent(tools=[*tools.tools], model=HfApiModel())
agent.run("Find all textbooks about machine learning in my library")
```

## Example Agentic Query

Once connected, an agent can run multi-step library queries:

1. `search_books(tags="machine-learning", document_type="textbook")` to get book IDs.
2. `list_chapters(book_id=<id>)` to identify relevant chapters.
3. `get_book_text(book_id=<id>, chapter=3)` to retrieve the chapter text.
4. The agent synthesizes an answer from the retrieved passages.

## Operational Notes

- `stdio` is the default transport and is the safest option for local desktop agents.
- `sse` is for HTTP clients and remote orchestration tools.
- `semantic_search` returns a tool error when LLM features are disabled. Enable `llm.enabled = true` in `config.toml` to use it.
- API tokens are admin-generated and can be revoked without affecting JWT login sessions.
