#!/usr/bin/env python3
"""Branch census of the out-of-line dalek_ff_group::field::FieldElement bodies in a
static archive. Reads `objdump -d -r -C -w --no-show-raw-insn <archive>` on stdin.
For each function whose demangled name mentions dalek_ff_group::field::FieldElement
as the receiver (`<dalek_ff_group::field::FieldElement as ...>::...` or
`<dalek_ff_group::field::FieldElement>::...`), print: member, instruction count,
conditional jumps with a classification of each, and the call targets.
A conditional jump is classed:
  panic-guard : its target block (up to the next jmp/ret/call) calls a core::panicking fn
  loop        : backward target inside the function
  other       : anything else (to be read by hand)
"""
import re, sys

FN = re.compile(r'^([0-9a-f]+) <(.*)>:$')
INS = re.compile(r'^\s*([0-9a-f]+):\s+([a-z][a-z0-9.]*)\s*(.*)$')
REL = re.compile(r'^\s*([0-9a-f]+):\s+(R_X86_64_\S+)\s+(.*)$')
MEMBER = re.compile(r'^(\S+\.o):\s+file format')
JCC = re.compile(r'^j(?!mp$)[a-z]+$')
TGT = re.compile(r'^([0-9a-f]+)\b')

def is_target(name):
    return (name.startswith('<dalek_ff_group::field::FieldElement as ')
            or name.startswith('<dalek_ff_group::field::FieldElement>::')
            or name.startswith('<&dalek_ff_group::field::FieldElement as '))

def report(member, name, ins):
    addrs = {a: i for i, (a, m, o, r) in enumerate(ins)}
    calls, jccs = [], []
    for i, (a, m, o, r) in enumerate(ins):
        if m.startswith('call'):
            calls.append(r or o)
        if JCC.match(m):
            t = TGT.match(o)
            ta = int(t.group(1), 16) if t else None
            cls = 'other'
            if ta is not None and ta <= a and ta in addrs:
                cls = 'loop'
            elif ta is not None and ta in addrs:
                k = addrs[ta]
                for (a2, m2, o2, r2) in ins[k:k + 16]:
                    if m2.startswith('call'):
                        if 'core::panicking' in (r2 or o2):
                            cls = 'panic-guard: ' + re.sub(r'-0x4$', '', (r2 or o2).split('::')[-1])
                        break
                    if m2 in ('ret', 'jmp') or m2.startswith('ud2'):
                        break
            jccs.append((a, m, cls))
    short = name.replace('dalek_ff_group::field::', '')
    print(f'{short}\n  member {member}, {len(ins)} instructions, {len(jccs)} conditional jumps')
    for a, m, cls in jccs:
        print(f'    {a:#x} {m:5s} {cls}')
    cs = sorted(set(re.sub(r'-0x4$', '', c) for c in calls))
    if cs:
        print('  calls: ' + '; '.join(cs))

member, name, ins = None, None, []
for line in sys.stdin:
    line = line.rstrip('\n')
    mm = MEMBER.match(line)
    if mm:
        member = mm.group(1)
        continue
    fm = FN.match(line)
    if fm:
        if name and is_target(name):
            report(member, name, ins)
        name, ins = fm.group(2), []
        continue
    if name is None:
        continue
    rm = REL.match(line)
    if rm and ins:
        a, m, o, _ = ins[-1]
        ins[-1] = (a, m, o, rm.group(3))
        continue
    im = INS.match(line)
    if im:
        ins.append((int(im.group(1), 16), im.group(2), im.group(3), None))
if name and is_target(name):
    report(member, name, ins)
