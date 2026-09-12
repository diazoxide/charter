# ASSERT THE PHRASE, NOT THE INTERPOLATED NAME. A test that checks only a

_2026-09-10 15:52 · persistent_

ASSERT THE PHRASE, NOT THE INTERPOLATED NAME. A test that checks only a value the message interpolates (assertIn('north', err) against "… still locked to 'north'.") cannot fail on the sentence: re-spelling the whole line leaves it green. Measured on #939 round 1, where the deletion sweep's [retune-string] operator would have charged it and a hand mutation found it first. The fix is to assert the surrounding words plus the value ('still locked to \'north\''). Same shape as the reviewer's other finding on that PR: charter announced '🔒 locked for this session' beside a workspace that was NOT the lock, because the message printed resolve()/args.name while is_locked() answers the chat's launch record — ADR 0013 says name the divergence.
