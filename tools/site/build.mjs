// Build the website: site/ → _site/, with copy pulled from the single sources it shares
// with the README (FAQ, disclaimer, release asset names), the brand assets and self-hosted Plex.
//
//   node tools/site/build.mjs        (or: npm run site --prefix tools)
//
// Template markers in site/**/*.html:
//   {{dl:KEY}}            static download URL (STATIC_LINKS)       KEY = mac-arm | mac-intel | windows | linux
//   {{asset:KEY}}         stable release file name (RELEASE_ASSETS)
//   <!--@disclaimer-->    ONE_LINE_DISCLAIMER, rendered as HTML
//   <!--@faq-->           every README FAQ item as <details>
//   <!--@faq-answer:Q-->  the README answer to question Q (for prose sections)
//   <!--@faqpage-->       FAQPage JSON-LD built from the page's own #faq <details>
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync, copyFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ONE_LINE_DISCLAIMER, RELEASE_ASSETS } from '../lib/constants.mjs';
import { STATIC_LINKS } from '../../site/js/downloads.js';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SRC = join(ROOT, 'site');
const OUT = join(ROOT, '_site');
const BLOB = 'https://github.com/n0dal/inkroot/blob/main/';

// Only the assets the pages use.
const ASSETS = [
  'brand/inkroot-mark.svg',
  'brand/inkroot-wordmark.svg',
  'brand/inkroot-wordmark-dark.svg',
  'icon/inkroot-icon-1024.png',
  'screenshots/home.png',
  'screenshots/root-step.png',
  'screenshots/done.png',
  'screenshots/rescue-cable.png',
  'social/social-preview.png',
];

// Plex Sans 400/600 and Mono 400/600: the Latin-1 subset (all body copy) plus the small
// "Pi" subset (arrows, primes), loaded only when a page uses those characters.
const FONTS = [];
for (const [pkg, family] of [['plex-sans', 'IBMPlexSans'], ['plex-mono', 'IBMPlexMono']]) {
  for (const weight of ['Regular', 'SemiBold']) {
    for (const subset of ['Latin1', 'Pi']) {
      FONTS.push(join(ROOT, 'tools/node_modules/@ibm', pkg, 'fonts/split/woff2', `${family}-${weight}-${subset}.woff2`));
    }
  }
}

// ---------- a tiny Markdown subset (what the README FAQ uses) ----------

const escapeHtml = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const escapeAttr = (s) => escapeHtml(s).replace(/"/g, '&quot;');
const siteUrl = (href) => (/^(https?:|mailto:|#)/.test(href) ? href : BLOB + href.replace(/^\.\//, ''));

export function inlineMd(src) {
  const code = [];
  let s = src.replace(/`([^`]+)`/g, (_, c) => `\u0000${code.push(c) - 1}\u0000`);
  s = escapeHtml(s)
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, t, u) => `<a href="${escapeAttr(siteUrl(u))}">${t}</a>`)
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*\w])\*([^*\s][^*]*?)\*(?!\*)/g, '$1<em>$2</em>');
  return s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${escapeHtml(code[+i])}</code>`);
}

export function blockMd(src) {
  const blocks = src.replace(/\r/g, '').trim().split(/\n\s*\n/);
  return blocks.map((block) => {
    const lines = block.split('\n');
    if (lines.every((l) => /^\s*>/.test(l))) {
      return `<blockquote>${blockMd(lines.map((l) => l.replace(/^\s*>\s?/, '')).join('\n'))}</blockquote>`;
    }
    // A block is optional paragraph lines, then optional "1." list items (indented lines continue an item).
    const firstItem = lines.findIndex((l) => /^\d+\.\s/.test(l));
    const para = firstItem === -1 ? lines : lines.slice(0, firstItem);
    const items = [];
    for (const l of firstItem === -1 ? [] : lines.slice(firstItem)) {
      if (/^\d+\.\s/.test(l)) items.push(l.replace(/^\d+\.\s+/, ''));
      else items[items.length - 1] += ' ' + l.trim();
    }
    return [
      para.length ? `<p>${inlineMd(para.map((l) => l.trim()).join(' '))}</p>` : '',
      items.length ? `<ol>${items.map((i) => `<li>${inlineMd(i)}</li>`).join('')}</ol>` : '',
    ].join('');
  }).join('\n');
}

/** Plain text of an HTML fragment, as a reader sees it (block boundaries become spaces). */
export function htmlToText(html) {
  return html
    .replace(/<\/?(p|li|ol|ul|blockquote|br|div|h\d)\b[^>]*>/gi, ' ')
    .replace(/<[^>]+>/g, '')
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

// ---------- README FAQ ----------

export function readmeFaq(readme) {
  const section = readme.split(/^## FAQ\s*$/m)[1]?.split(/^## /m)[0];
  if (!section) throw new Error('README.md has no "## FAQ" section');
  const items = [...section.matchAll(/<details>\s*<summary><b>([\s\S]*?)<\/b><\/summary>([\s\S]*?)<\/details>/g)]
    .map(([, q, a]) => ({ question: q.trim(), html: blockMd(a.replace(/^\s*\*\*[^*\n]+\*\*[ \t]*\n/, '')) }));
  if (!items.length) throw new Error('README.md FAQ has no <details> items');
  return items;
}

const faqDetails = (items) => items.map(({ question, html }) =>
  `<details>\n<summary>${escapeHtml(question)}</summary>\n<div class="answer">\n${html}\n</div>\n</details>`).join('\n');

/** FAQPage JSON-LD from the <details> inside the page's <section id="faq">. */
export function faqPageJsonLd(page) {
  const faq = page.match(/<section[^>]*\bid="faq"[^>]*>([\s\S]*?)<\/section>/)?.[1];
  if (!faq) throw new Error('<!--@faqpage--> used on a page without <section id="faq">');
  const mainEntity = [...faq.matchAll(/<details[^>]*>\s*<summary[^>]*>([\s\S]*?)<\/summary>([\s\S]*?)<\/details>/g)]
    .map(([, q, a]) => ({
      '@type': 'Question',
      name: htmlToText(q),
      acceptedAnswer: { '@type': 'Answer', text: htmlToText(a) },
    }));
  const json = JSON.stringify({ '@context': 'https://schema.org', '@type': 'FAQPage', mainEntity }, null, 2);
  return `<script type="application/ld+json">\n${json.replace(/</g, '\\u003c')}\n</script>`;
}

// ---------- pages ----------

function renderPage(html, faq, file) {
  const out = html
    .replace(/\{\{dl:([\w-]+)\}\}/g, (m, k) => STATIC_LINKS[k] ?? fail(`${file}: unknown download key ${k}`))
    .replace(/\{\{asset:([\w-]+)\}\}/g, (m, k) => RELEASE_ASSETS[k] ?? fail(`${file}: unknown asset key ${k}`))
    .replace(/<!--@disclaimer-->/g, () => inlineMd(ONE_LINE_DISCLAIMER))
    .replace(/<!--@faq-->/g, () => faqDetails(faq))
    .replace(/<!--@faq-answer:(.+?)-->/g, (m, q) =>
      (faq.find((i) => i.question === q.trim()) ?? fail(`${file}: no README FAQ question "${q}"`)).html);
  const done = out.replace(/<!--@faqpage-->/g, () => faqPageJsonLd(out));
  const left = done.match(/\{\{[^}]*\}\}|<!--@[^>]*-->/);
  if (left) fail(`${file}: unreplaced template marker ${left[0]}`);
  return done;
}

function fail(msg) {
  throw new Error(msg);
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]);
}

export function build() {
  const faq = readmeFaq(readFileSync(join(ROOT, 'README.md'), 'utf8'));
  rmSync(OUT, { recursive: true, force: true });

  for (const src of walk(SRC)) {
    const rel = relative(SRC, src);
    const dest = join(OUT, rel);
    mkdirSync(dirname(dest), { recursive: true });
    if (rel === '.htmlvalidate.json') continue; // validator config stays in the repo, not the deployed artifact
    if (rel.endsWith('.html')) {
      writeFileSync(dest, renderPage(readFileSync(src, 'utf8'), faq, rel));
    } else if (rel === join('js', 'downloads.js')) {
      // In the repo the module imports the shared constants from tools/lib; on the site they sit beside it.
      const js = readFileSync(src, 'utf8');
      const from = "from '../../tools/lib/constants.mjs'";
      if (!js.includes(from)) fail(`site/js/downloads.js no longer contains ${from}`);
      writeFileSync(dest, js.replace(from, "from './constants.mjs'"));
      copyFileSync(join(ROOT, 'tools/lib/constants.mjs'), join(OUT, 'js/constants.mjs'));
    } else {
      copyFileSync(src, dest);
    }
  }

  for (const a of ASSETS) {
    const dest = join(OUT, 'assets', a);
    mkdirSync(dirname(dest), { recursive: true });
    copyFileSync(join(ROOT, 'assets', a), dest);
  }

  mkdirSync(join(OUT, 'fonts'), { recursive: true });
  for (const f of FONTS) {
    if (!existsSync(f)) fail(`missing font ${relative(ROOT, f)} (run npm ci --prefix tools)`);
    cpSync(f, join(OUT, 'fonts', f.split('/').pop()));
  }

  // IBM Plex is OFL-licensed: ship its licence text beside the fonts.
  const ofl = join(ROOT, 'tools/node_modules/@ibm/plex-sans/LICENSE.txt');
  if (!existsSync(ofl)) fail('missing IBM Plex licence file (run npm ci --prefix tools)');
  copyFileSync(ofl, join(OUT, 'fonts/OFL.txt'));

  return walk(OUT).length;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    const n = build();
    console.log(`site built: ${n} files in ${relative(process.cwd(), OUT) || '.'}`);
  } catch (e) {
    console.error(`site build failed: ${e.message}`);
    process.exit(1);
  }
}
