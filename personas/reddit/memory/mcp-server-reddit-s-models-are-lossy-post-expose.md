# mcp-server-reddit's models are lossy: Post exposes only id/title/author/

_2026-08-14 22:21 · persistent_

mcp-server-reddit's models are lossy: Post exposes only id/title/author/score/subreddit/url/created_at/comment_count/post_type/content — NO upvote_ratio, NO flair, and NO removed_by_category/banned_by, so it cannot tell you whether a post was removed by AutoMod or a mod. SubredditInfo returns only name/subscriber_count/public_description — it cannot read subreddit rules. Moderation status and rules checks need a different server or a human with a browser.
