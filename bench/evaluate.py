import json, os, random, hashlib

BASE = os.path.dirname(os.path.abspath(__file__))
spec = json.load(open(os.path.join(BASE, "testset.json"), encoding="utf-8"))
items = {i["id"]: i for i in spec["items"]}
gloss = spec["glossary"]
tiers = ["lite", "default", "reasoning"]

outs = {}
for t in tiers:
    p = os.path.join(BASE, f"out_{t}.json")
    outs[t] = {r["id"]: r["target"] for r in json.load(open(p, encoding="utf-8"))}

print("=" * 62)
print("A. 术语一致性（客观可测，不依赖人工判断）")
print("=" * 62)
total_req = 0
rows = []
for t in tiers:
    hit = miss = 0
    detail = []
    for iid, it in items.items():
        if not it["must_terms"]:
            continue
        tr = outs[t].get(iid, "")
        for term in it["must_terms"]:
            total_req += 1
            if term in tr:
                hit += 1
            else:
                miss += 1
                detail.append(f"{iid}:未命中「{term}」")
    rows.append((t, hit, miss, detail))

for t, hit, miss, detail in rows:
    tot = hit + miss
    print(f"  {t:10s}  {hit}/{tot}  = {hit/tot*100:.0f}%" if tot else f"  {t}: n/a")
    for d in detail:
        print(f"      - {d}")

print()
print("=" * 62)
print("B. 译文预览（人工可复核）")
print("=" * 62)
for iid in ["news-01", "tech-02", "legal-01", "colloq-03", "idiom-01", "idiom-03"]:
    it = items[iid]
    print(f"\n[{iid}] {it['domain']} {it['direction']}")
    print(f"  原文: {it['source']}")
    for t in tiers:
        print(f"  {t:10s}: {outs[t].get(iid,'<缺失>')}")

# 生成盲评包：随机打乱 A/B/C 映射
seed = "deepl-bench-2026"
names = tiers[:]
random.Random(seed).shuffle(names)
blind = {}
for idx, real in enumerate(names):
    blind[chr(65 + idx)] = real

blind_items = []
for iid, it in items.items():
    blind_items.append({
        "id": iid,
        "source": it["source"],
        "direction": it["direction"],
        "must_terms": it["must_terms"],
        "A": outs[blind["A"]].get(iid, ""),
        "B": outs[blind["B"]].get(iid, ""),
        "C": outs[blind["C"]].get(iid, ""),
    })
json.dump(blind_items, open(os.path.join(BASE, "blind_pack.json"), "w", encoding="utf-8"),
          ensure_ascii=False, indent=1)
json.dump({"mapping": blind}, open(os.path.join(BASE, "blind_key.json"), "w", encoding="utf-8"),
          ensure_ascii=False, indent=1)
print("\n盲评包已生成: blind_pack.json （映射见 blind_key.json，评审方不得读取）")
