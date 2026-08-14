# personas/reddit/mcp.json pins nothing, so 'uvx mcp-server-reddit' resolv

_2026-08-14 19:23 · persistent_

personas/reddit/mcp.json pins nothing, so 'uvx mcp-server-reddit' resolves mcp>=2.0.0 and the server crashes at import (ImportError: cannot import name 'McpError' from mcp.shared.exceptions — renamed MCPError in mcp 2.0.0). The mcp__reddit__* tools are therefore absent from sub-agent sessions. Fix: pin the SDK, e.g. args ['--from','mcp-server-reddit','--with','mcp<2','mcp-server-reddit'].
