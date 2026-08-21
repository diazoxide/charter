# util.run passes stdin=DEVNULL for every caller (conditional on input=Non

_2026-08-21 23:18 · persistent_

util.run passes stdin=DEVNULL for every caller (conditional on input=None, since subprocess.run rejects being handed both). Safe because NO caller passes capture=False — every call already captures stdout+stderr, so none is interactive — and charter's one interactive path, secret --exec, uses os.execvpe and never routes through util.run. Never 'fix' this by reverting to inherited stdin: gh reads stdin for a field value naming it (#323), which hung a status refresh (#324).
