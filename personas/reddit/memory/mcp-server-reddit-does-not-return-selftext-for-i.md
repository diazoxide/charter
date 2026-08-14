# mcp-server-reddit does not return selftext for image/media posts: get_po

_2026-08-14 22:31 · persistent_

mcp-server-reddit does not return selftext for image/media posts: get_post_content sets post_type='link' and content=the post's own permalink, so the body is unverifiable. Plain self-posts return post_type='text' with real body text. Never claim a media post's body is intact from this tool.
