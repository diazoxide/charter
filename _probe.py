
import itertools, sys
sys.path.insert(0, ".")
from charter import hooks
out=[]
for L in range(0,7):
    for t in itertools.product('"\'`$(\\<\n-B x', repeat=L):
        out.append(repr(hooks._live_substitution("".join(t))))
print("\x00".join(out))
