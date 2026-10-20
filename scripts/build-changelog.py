#!/usr/bin/env python3
"""Build docs/changelog.html from release-notes/v*.md, dated from git tags.
Run after shipping a version: python3 scripts/build-changelog.py"""
import html, re, subprocess, pathlib, datetime

ROOT = pathlib.Path(__file__).resolve().parent.parent
notes = sorted((p for p in (ROOT / 'release-notes').glob('v*.md')),
               key=lambda p: [int(x) for x in p.stem[1:].split('.')], reverse=True)

def tag_date(tag):
    try:
        out = subprocess.run(['git', 'log', '-1', '--format=%ad', '--date=short', tag],
                             cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()
        return out or None
    except subprocess.CalledProcessError:
        return None

entries = []
for p in notes:
    ver = p.stem
    text = p.read_text().strip()
    date = tag_date(ver)
    paras = [html.escape(t.strip()) for t in re.split(r'\n\s*\n', text) if t.strip()]
    entries.append((ver, date, paras))

def iso(d): return d or ''
def nice(d):
    return datetime.date.fromisoformat(d).strftime('%-d %B %Y') if d else ''

items = []
for ver, date, paras in entries:
    body = ''.join(f'<p>{t}</p>' for t in paras)
    items.append(f'''      <article class="entry" id="{ver}">
        <header>
          <h2><a href="#{ver}">{ver}</a></h2>
          {f'<time datetime="{iso(date)}">{nice(date)}</time>' if date else ''}
          <a class="dl" href="https://github.com/soundrich-bot/laybacker/releases/tag/{ver}">download</a>
        </header>
        {body}
      </article>''')

latest = entries[0][0] if entries else ''
latest_date = entries[0][1] if entries else ''
page = f'''<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Laybacker changelog — what changed in each version</title>
  <meta name="description" content="Every Laybacker release with what changed: slates, loudness and QC checks, the Audio Only page, the Multifunction Chain, themes and fixes. Latest: {latest}." />
  <link rel="canonical" href="https://laybacker.com/changelog.html" />
  <meta property="og:url" content="https://laybacker.com/changelog.html" />
  <meta property="og:title" content="Laybacker changelog" />
  <meta property="og:description" content="What changed in each Laybacker release. Latest: {latest}." />
  <meta property="og:type" content="website" />
  <script type="application/ld+json">
  {{
    "@context": "https://schema.org",
    "@type": "WebPage",
    "@id": "https://laybacker.com/changelog.html",
    "name": "Laybacker changelog",
    "description": "What changed in each Laybacker release.",
    "dateModified": "{iso(latest_date)}",
    "isPartOf": {{ "@type": "WebSite", "url": "https://laybacker.com/", "name": "Laybacker" }},
    "about": {{ "@id": "https://laybacker.com/#app" }}
  }}
  </script>
  <style>
    :root {{
      --bg-dark: #08080c; --bg-panel: #11121a; --border: #23252f;
      --text: #eef0f6; --text-2: #aeb2c0; --text-mut: #6b7080;
      --pink: #ff2e63; --cyan: #08f7fe;
      --mono: ui-monospace, "SF Mono", "Cascadia Code", "Roboto Mono", Menlo, Consolas, monospace;
      --disp: "Helvetica Neue", Arial, system-ui, sans-serif;
    }}
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    html {{ scroll-behavior: smooth; }}
    body {{ background: radial-gradient(1200px 600px at 50% -10%, #15131f 0%, var(--bg-dark) 60%) fixed; color: var(--text); font-family: var(--mono); line-height: 1.65; -webkit-font-smoothing: antialiased; min-height: 100vh; }}
    .wrap {{ max-width: 760px; margin: 0 auto; padding: 0 24px 64px; }}
    header.top {{ padding: 56px 0 28px; }}
    .logo {{ font-family: var(--disp); font-weight: 900; font-size: 22px; letter-spacing: 0.14em; }}
    .logo a {{ color: var(--text); text-decoration: none; }}
    .logo a span {{ background: linear-gradient(90deg, var(--pink), var(--cyan)); -webkit-background-clip: text; background-clip: text; color: transparent; }}
    h1 {{ font-family: var(--disp); font-size: 28px; margin-top: 18px; letter-spacing: 0.01em; }}
    .lead {{ color: var(--text-2); font-size: 14px; margin-top: 8px; max-width: 560px; }}
    .lead a {{ color: var(--cyan); text-decoration: none; border-bottom: 1px solid rgba(8,247,254,0.3); }}
    .entry {{ border: 1px solid var(--border); border-radius: 12px; background: var(--bg-panel); padding: 20px 22px; margin: 14px 0; }}
    .entry header {{ display: flex; align-items: baseline; gap: 14px; flex-wrap: wrap; margin-bottom: 10px; }}
    .entry h2 {{ font-family: var(--disp); font-size: 18px; letter-spacing: 0.02em; }}
    .entry h2 a {{ color: var(--text); text-decoration: none; }}
    .entry h2 a:hover {{ color: var(--cyan); }}
    .entry time {{ color: var(--text-mut); font-size: 12.5px; }}
    .entry .dl {{ margin-left: auto; color: var(--cyan); font-size: 12px; text-decoration: none; border-bottom: 1px solid rgba(8,247,254,0.3); }}
    .entry p {{ color: var(--text-2); font-size: 13.5px; }}
    .entry p + p {{ margin-top: 8px; }}
    .entry:target {{ border-color: var(--cyan); }}
    footer {{ color: var(--text-mut); font-size: 12px; margin-top: 40px; }}
    footer a {{ color: var(--cyan); text-decoration: none; }}
  </style>
</head>
<body>
  <div class="wrap">
    <header class="top">
      <div class="logo"><a href="/"><span>LAYBACKER</span></a></div>
      <h1>Changelog</h1>
      <p class="lead">What changed in each version, in the words it shipped with. Downloads for every release are on <a href="https://github.com/soundrich-bot/laybacker/releases">GitHub</a>; the newest is always at <a href="https://github.com/soundrich-bot/laybacker/releases/latest">releases/latest</a>.</p>
    </header>
{chr(10).join(items)}
    <footer>Laybacker is free and open source (GPL-3.0). <a href="/">Back to laybacker.com</a> · <a href="/llms.txt">Plain-text summary</a></footer>
  </div>
</body>
</html>
'''
(ROOT / 'docs' / 'changelog.html').write_text(page)
print(f'wrote docs/changelog.html with {len(entries)} releases (latest {latest}, {latest_date})')
