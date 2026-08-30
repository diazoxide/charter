# GitHub required status checks: a required check whose conclusion is 'ski

_2026-08-30 10:39 · persistent_

GitHub required status checks: a required check whose conclusion is 'skipped' counts as SATISFIED (PR reports mergeStateStatus CLEAN). So requiring a job that is skipped on pull_request — like charter's 'A dev-channel install from main actually installs', whose if: restricts it to push-to-main — is pure decoration and reintroduces #561's shape. A required context that NO job ever reports is the opposite: permanently BLOCKED. Measured 2026-08-30.
