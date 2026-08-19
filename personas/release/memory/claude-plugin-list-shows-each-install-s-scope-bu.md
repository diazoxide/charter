# `claude plugin list` shows each install's SCOPE but never which project

_2026-08-19 00:05 · persistent_

`claude plugin list` shows each install's SCOPE but never which project a project-scope install belongs to — it just repeats identical-looking entries. Read ~/.claude/plugins/installed_plugins.json and use each entry's projectPath (and installPath, to confirm the cache dir really holds that version) before reporting where a stale install lives. At 0.44.1 I inferred the two stale project-scope installs were workspace clones under the plane, by grepping .claude/settings.json for 'charter@charter' — wrong question: that finds projects that ENABLE the plugin, not projects with a registered install. They were actually two unrelated repos, easydmarc-umbrella and volaticloud. Getting this wrong sends someone to the wrong repo, and the fix touches other people's projects rather than this plane's internals.
