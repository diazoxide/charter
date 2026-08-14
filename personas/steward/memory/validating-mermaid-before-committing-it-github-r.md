# Validating mermaid before committing it (GitHub renders a syntax error a

_2026-08-14 13:47 · persistent_

Validating mermaid before committing it (GitHub renders a syntax error as a red error box): the official CLI works offline-ish against the already-installed Chrome — write puppeteer.json {"executablePath":"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome","args":["--no-sandbox"]} then PUPPETEER_SKIP_DOWNLOAD=1 npx -y @mermaid-js/mermaid-cli@11 -p puppeteer.json -i x.mmd -o x.svg. Extract the fenced blocks from the README and validate them AS EMBEDDED, not the sources they came from.
