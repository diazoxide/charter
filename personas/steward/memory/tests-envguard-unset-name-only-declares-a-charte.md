# tests/_envguard.unset(name) only DECLARES a charter variable absent for

_2026-09-10 14:01 · persistent_

tests/_envguard.unset(name) only DECLARES a charter variable absent for the ambient-read guard; it does NOT remove it from os.environ. A case inside a class whose setUp did mock.patch.dict(os.environ, {'CHARTER_SESSION_ID': ...}) that calls _envguard.unset('CHARTER_SESSION_ID') still runs IN-FRAME, silently. Put an outside-a-frame case in its own PersonaIso/PlaneIso class (their setUp scrubs every charter variable) instead. Found writing #936's fallback test, which briefed the chat's workspace instead of default.
