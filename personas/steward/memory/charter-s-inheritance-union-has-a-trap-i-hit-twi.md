# Charter's inheritance union has a trap I hit twice on 2026-08-19: person

_2026-08-19 18:05 · persistent_

Charter's inheritance union has a trap I hit twice on 2026-08-19: persona.lineage() is CHILD-FIRST ([name, parent, grandparent]). So 'for anc in lineage(name): out.update(x)' makes the most DISTANT ancestor win — the opposite of the documented 'child wins' rule. mcp_servers() had this bug live (filed #296, fixed in PR 297); bin_scripts() uses reversed(lineage()). Any new lineage union MUST iterate reversed(lineage(name)). Also verified: toolgate._parse() reduces a command to os.path.basename, so 'tools: foo.sh' auto-approves ANY foo.sh anywhere unless a provenance check pins it to the persona's own bin/ (PR 295).
