
import sys, timeit; sys.path.insert(0, ".")
from charter import hooks
TYP  = "git status --porcelain && python3 -m unittest discover -s tests"
BODY = "gh issue comment 1 --body-file - <<'B'\n" + ("plain prose line here\n"*300) + "B"
DQ   = 'gh issue create --body "' + ("a $ b " * 300) + '"'
for n, c, k in (("typical", TYP, 5000), ("heredoc", BODY, 400), ("dollars", DQ, 400)):
    print(f"{n}:{timeit.timeit(lambda: hooks._live_substitution(c), number=k)/k*1e6:.2f}", end="  ")
print()
