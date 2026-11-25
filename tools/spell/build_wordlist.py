"""Build desktop/assets/spell/words.txt from the SCOWL-derived Hunspell
en_US dictionary (Debian package hunspell-en-us, SCOWL 2020.12.07).

Usage: python3 tools/spell/build_wordlist.py [/usr/share/hunspell/en_US]

The stems are expanded with the affix rules of the .aff file (prefixes,
suffixes and their cross products). Words with digits, other symbols or a
length of one are dropped, and so are possessives (the checker strips
's itself). The output is sorted and front coded: each line
is the number of leading characters shared with the previous word (one
character, 0-9 then a-z for 10-35), followed by the rest of the word.
"""
import re
import sys
from pathlib import Path

base = Path(sys.argv[1] if len(sys.argv) > 1 else "/usr/share/hunspell/en_US")
aff = base.with_suffix(".aff").read_text(encoding="utf-8").splitlines()
dic = base.with_suffix(".dic").read_text(encoding="utf-8").splitlines()[1:]

rules = {}  # flag -> (kind, cross, [(strip, add, regex)])
i = 0
while i < len(aff):
    parts = aff[i].split()
    if len(parts) == 4 and parts[0] in ("PFX", "SFX") and parts[2] in ("Y", "N"):
        kind, flag, cross, count = parts[0], parts[1], parts[2] == "Y", int(parts[3])
        entries = []
        for line in aff[i + 1 : i + 1 + count]:
            p = line.split()
            strip = "" if p[2] == "0" else p[2]
            add = p[3].split("/")[0]
            add = "" if add == "0" else add
            cond = p[4] if len(p) > 4 else "."
            pattern = ("^" + cond) if kind == "PFX" else (cond + "$")
            entries.append((strip, add, re.compile(pattern)))
        rules[flag] = (kind, cross, entries)
        i += count + 1
    else:
        i += 1


def apply(word, flag):
    kind, cross, entries = rules[flag]
    out = []
    for strip, add, cond in entries:
        if not cond.search(word):
            continue
        if kind == "SFX":
            if strip and not word.endswith(strip):
                continue
            out.append((word[: len(word) - len(strip)] if strip else word) + add)
        else:
            if strip and not word.startswith(strip):
                continue
            out.append(add + (word[len(strip) :] if strip else word))
    return out


words = set()
for line in dic:
    stem, _, flags = line.partition("/")
    stem = stem.strip()
    if not stem:
        continue
    flags = [f for f in flags.strip() if f in rules]
    words.add(stem)
    sfx = [f for f in flags if rules[f][0] == "SFX"]
    pfx = [f for f in flags if rules[f][0] == "PFX"]
    suffixed = []
    for f in sfx:
        made = apply(stem, f)
        words.update(made)
        if rules[f][1]:
            suffixed.extend(made)
    for f in pfx:
        words.update(apply(stem, f))
        if rules[f][1]:
            for w in suffixed:
                words.update(apply(w, f))

good = re.compile(r"^[A-Za-z][A-Za-z']*[A-Za-z]$")
kept = sorted(w for w in words if good.match(w) and not w.endswith("'s"))
digits = "0123456789abcdefghijklmnopqrstuvwxyz"
lines = []
prev = ""
for w in kept:
    n = 0
    while n < min(len(prev), len(w), 35) and prev[n] == w[n]:
        n += 1
    lines.append(digits[n] + w[n:])
    prev = w
out = Path(__file__).resolve().parents[2] / "desktop/assets/spell/words.txt"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"{len(kept)} words, {out.stat().st_size} bytes -> {out}")
