#!/usr/bin/env python3
"""Extract and filter Pattaya condo listings 70K-110K THB from PropertyHub."""
import re, json

with open('/tmp/condo_ph.html') as f:
    html = f.read()

nm = re.search(r'<script id="__N(?:EXT|UXT)_DATA__"[^>]*>(.*?)</script>', html)
nd = json.loads(nm.group(1))
listings_raw = nd['props']['pageProps']['resultListings']

def extract_price(item):
    p = item.get('price', {})
    if isinstance(p, dict):
        rent = p.get('forRent', {})
        if isinstance(rent, dict):
            monthly = rent.get('monthly', {})
            if isinstance(monthly, dict):
                return monthly.get('price', 0)
    return 0

results = []
for item in listings_raw:
    price = extract_price(item)
    if price == 0:
        continue
    
    project = item.get('project', {})
    location = item.get('location', {})
    
    results.append({
        'title': item.get('title', 'N/A'),
        'price': price,
        'building': project.get('nameEnglish') or project.get('name', 'N/A'),
        'slug': item.get('slug', ''),
        'bedrooms': item.get('bedroom', 'N/A'),
        'bathrooms': item.get('bathroom', 'N/A'),
        'area': item.get('livingSize', item.get('landSize', 'N/A')),
        'floor': item.get('floor', 'N/A'),
        'property_type': item.get('propertyType', ''),
        'url': f"https://propertyhub.in.th/en/condo-for-rent/{item.get('slug', '')}",
        'image': item.get('coverPicture', ''),
    })

# Filter
results.sort(key=lambda x: x['price'])
in_budget = [r for r in results if 70000 <= r['price'] <= 110000]

print(f"Total listings with prices: {len(results)}")
print(f"In range ฿70K-110K: {len(in_budget)}")
print()

if in_budget:
    print("=" * 70)
    print("CONDOS IN ฿70,000 - ฿110,000/MONTH RANGE")
    print("=" * 70)
    for i, r in enumerate(in_budget, 1):
        print(f"\n--- #{i} ---")
        print(f"  Title: {r['title']}")
        print(f"  Price: ฿{r['price']:,}/month")
        print(f"  Building: {r['building']}")
        print(f"  Area: {r['area']} sqm | {r['bedrooms']}BR | {r['bathrooms']}BA")
        print(f"  Floor: {r['floor']}")
        print(f"  URL: {r['url']}")

# Also show near-miss (50K-70K and 110K-150K)
near = [r for r in results if 50000 <= r['price'] <= 150000 and r not in in_budget]
if near:
    print(f"\n{'=' * 70}")
    print("NEAR-MISS LISTINGS (฿50K-฿150K, for context)")
    print("=" * 70)
    for r in sorted(near, key=lambda x: x['price']):
        tag = "↑ just under" if r['price'] < 70000 else "↓ just over"
        print(f"  ฿{r['price']:,} {tag} | {r['bedrooms']}BR | {r['area']}sqm | {r['building'][:40]} | {r['title'][:50]}")

print(f"\nSummary: {len(in_budget)} condo(s) exactly in ฿70K-110K range")
