// The driver for tests/test_opencode_shim_dispatches_at_runtime.py.
//
// A stand-in for Bun's `$` that records the COMMAND LINE the shim actually builds, so the
// test can assert the call site rather than the routing table. #433 shipped for four
// releases with the tables right there in the file and no call site reading them, and the
// round-one fix was still mutable back to a literal `charter hook pretooluse` with the
// suite green — because every assertion looked at the table.
//
// Reads scenarios as JSON on stdin, writes one result per scenario as JSON on stdout.
// Nothing here interprets a command; it records what was run and Python decides.
import { CharterPlugin } from './charter.mjs';

const scenarios = JSON.parse(await new Promise((res) => {
  let buf = ''; process.stdin.setEncoding('utf8');
  process.stdin.on('data', (d) => { buf += d; });
  process.stdin.on('end', () => res(buf));
}));

const results = [];

for (const s of scenarios) {
  // Not a shim call: the RUNTIME's own list of inherited property names, so the test can
  // exercise every id a plain `TABLE[key]` would resolve through the prototype chain
  // without keeping a copy of that list in Python — including whichever names a future
  // JS engine adds to `Object.prototype`.
  if (s.event === 'protokeys') {
    results.push({ keys: Object.getOwnPropertyNames(Object.prototype) });
    continue;
  }
  const calls = [];
  const $ = (strings, ...values) => {
    let command = '';
    let blob = null;
    for (let i = 0; i < strings.length; i++) {
      command += strings[i];
      if (i < values.length) {
        const v = values[i];
        if (v && typeof v.text === 'function') { blob = v; command += '<stdin>'; }
        else command += String(v);
      }
    }
    const rec = { command, payload: null, env: null };
    const chain = {
      env(e) {
        // Only charter's own keys are projected out — never the whole environment.
        rec.env = { CHARTER_HARNESS: e.CHARTER_HARNESS ?? null,
                    CHARTER_SESSION_ID: e.CHARTER_SESSION_ID ?? null };
        return chain;
      },
      quiet() { chain._quiet = true; return chain; },
      nothrow() { chain._nothrow = true; return chain; },
      then(ok, no) {
        return (async () => {
          if (blob) { try { rec.payload = JSON.parse(await blob.text()); } catch { rec.payload = null; } }
          calls.push(rec);
          return { stdout: Buffer.from(s.reply ?? '') };
        })().then(ok, no);
      },
    };
    return chain;
  };

  const hooks = await CharterPlugin({ $, directory: s.directory ?? '/plane' });
  const out = { calls, threw: null, output: null, env: null };
  const input = { tool: s.tool, sessionID: s.sessionID ?? '', callID: 'c1' };
  try {
    if (s.event === 'before') {
      await hooks['tool.execute.before'](input, { args: s.args ?? {} });
    } else if (s.event === 'after') {
      const o = { output: s.output ?? '', title: '', metadata: {} };
      input.args = s.args ?? {};
      await hooks['tool.execute.after'](input, o);
      out.output = o.output;
    } else if (s.event === 'shellenv') {
      const o = { env: {} };
      await hooks['shell.env']({ sessionID: s.sessionID ?? '', cwd: '/plane' }, o);
      out.env = { CHARTER_HARNESS: o.env.CHARTER_HARNESS ?? null,
                  CHARTER_SESSION_ID: o.env.CHARTER_SESSION_ID ?? null };
    }
  } catch (e) { out.threw = String(e && e.message); }
  results.push(out);
}

process.stdout.write(JSON.stringify(results));
