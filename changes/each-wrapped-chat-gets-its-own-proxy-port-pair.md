### Changed

- **Each Codex and opencode chat gets its own pair of proxy ports, HTTP and SOCKS5.** purlis's
  proxy beside the chat now also speaks SOCKS5. A chat's sandbox lets it reach only its own two
  ports, so no chat can use another chat's. The chat a connection belongs to is the port it
  came in on, never anything the connection sends. Both ports close when the chat ends.
- **The proxy refuses this machine, link-local addresses and cloud metadata services**, both
  as addresses and as whatever a name resolves to. The only exception is that exact address
  and port, if it is listed. The refusal is a Block whose Notice does not offer an Allow,
  because allowing the host would not let it through.
- **Every connection the proxy carries or refuses is now in the network record.** The record
  keeps the host and port, how it was decided (an Open host, a Persona host, an Allowed host,
  or refused), and a count. Connections are grouped into one line per host and port each
  minute, so the record stays small.
