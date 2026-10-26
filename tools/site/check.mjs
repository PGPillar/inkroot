// Check the built site (_site/) for the SEO, legal and consistency rules the pages must keep.
//
//   node tools/site/check.mjs        (or: npm run check-site --prefix tools)
//
// Prints "site OK: <n> pages" or lists every problem and exits 1.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const OUT = join(ROOT, '_site');
const BASE = 'https://n0dal.github.io/inkroot/';
const NOT_AFFILIATED = 'Inkroot is not affiliated with, endorsed by, or sponsored by Onyx International / BOOX, Qualcomm, Google or the Magisk project.';
const PRIVACY = 'Inkroot collects and sends nothing — no analytics, no telemetry, no account. Your safety copies stay on your computer and are never uploaded.';

const problems = [];
const problem = (file, msg) => problems.push(`${file}: ${msg}`);

const walk = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
  e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]);

/** Visible text of an HTML fragment: block boundaries become spaces, entities decoded, whitespace collapsed. */
function text(html) {
  return html
    .replace(/<\/?(p|li|ol|ul|blockquote|br|div|h\d)\b[^>]*>/gi, ' ')
    .replace(/<[^>]+>/g, '')
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

const metaContent = (html, attr, name) =>
  html.match(new RegExp(`<meta\\s+${attr}="${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}"\\s+content="([^"]*)"`))?.[1];

function checkFaq(file, html, blocks) {
  const pages = blocks.filter((b) => b['@type'] === 'FAQPage');
  const section = html.match(/<section[^>]*\bid="faq"[^>]*>([\s\S]*?)<\/section>/)?.[1];
  if (!pages.length && !section) return;
  if (pages.length !== 1) return problem(file, `expected one FAQPage JSON-LD block, found ${pages.length}`);
  if (!section) return problem(file, 'FAQPage JSON-LD but no <section id="faq">');
  const visible = [...section.matchAll(/<details[^>]*>\s*<summary[^>]*>([\s\S]*?)<\/summary>([\s\S]*?)<\/details>/g)]
    .map(([, q, a]) => ({ q: text(q), a: text(a) }));
  const questions = pages[0].mainEntity ?? [];
  if (questions.length !== visible.length) {
    return problem(file, `FAQPage has ${questions.length} questions but #faq has ${visible.length} <details>`);
  }
  questions.forEach((q, i) => {
    if (text(q.name ?? '') !== visible[i].q) problem(file, `FAQ ${i + 1} question differs: "${q.name}" vs "${visible[i].q}"`);
    if (text(q.acceptedAnswer?.text ?? '') !== visible[i].a) problem(file, `FAQ ${i + 1} ("${visible[i].q}") answer differs from the visible answer`);
  });
  return visible.length;
}

function checkPage(file, html) {
  const canon = [...html.matchAll(/<link\s+rel="canonical"\s+href="([^"]*)"/g)].map((m) => m[1]);
  if (canon.length !== 1) problem(file, `expected exactly one canonical link, found ${canon.length}`);
  else if (!canon[0].startsWith(BASE)) problem(file, `canonical ${canon[0]} is not under ${BASE}`);
  if (metaContent(html, 'property', 'og:url') !== canon[0]) problem(file, 'og:url does not equal the canonical URL');

  if (!metaContent(html, 'property', 'og:image')) problem(file, 'missing og:image');
  if (!metaContent(html, 'name', 'twitter:card')) problem(file, 'missing twitter:card');
  if (!/<title>[^<]+<\/title>/.test(html)) problem(file, 'missing <title>');
  if (!metaContent(html, 'name', 'description')) problem(file, 'missing meta description');

  const blocks = [];
  for (const [, json] of html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)) {
    try {
      blocks.push(JSON.parse(json));
    } catch (e) {
      problem(file, `JSON-LD does not parse: ${e.message}`);
    }
  }
  const faqCount = checkFaq(file, html, blocks);
  if (file === 'index.html') {
    if (!blocks.some((b) => b['@type'] === 'SoftwareApplication')) problem(file, 'missing SoftwareApplication JSON-LD');
    if (!faqCount) problem(file, 'index has no FAQ');
    if (!html.includes('How do I turn on USB debugging on a BOOX?')) problem(file, 'FAQ lacks the USB-debugging question');
  }

  checkFooter(file, html);
  return canon[0];
}

function checkFooter(file, html) {
  const footer = html.match(/<footer[\s\S]*?<\/footer>/)?.[0] ?? '';
  if (!/href="[^"]*DISCLAIMER\.md"/.test(footer)) problem(file, 'footer lacks the disclaimer link');
  if (!text(footer).includes(NOT_AFFILIATED)) problem(file, 'footer lacks the not-affiliated line');
  if (!text(footer).includes(PRIVACY)) problem(file, 'footer lacks the privacy statement');
}

function main() {
  if (!existsSync(OUT)) {
    console.error('site check failed: _site/ does not exist (run npm run site --prefix tools first)');
    process.exit(1);
  }
  const files = walk(OUT);
  const htmlFiles = files.filter((f) => f.endsWith('.html'));
  const canonicals = [];

  for (const f of htmlFiles) {
    const rel = relative(OUT, f);
    const html = readFileSync(f, 'utf8');
    for (const img of html.match(/<img\b[^>]*>/g) ?? []) if (!/\balt="/.test(img)) problem(rel, `<img> without alt: ${img}`);
    const marker = html.match(/\{\{[^}]*\}\}|<!--@[^>]*-->/);
    if (marker) problem(rel, `unreplaced template marker ${marker[0]}`);
    if (rel === '404.html') {
      if (!/<meta name="robots" content="noindex">/.test(html)) problem(rel, '404 page must be noindex');
      if (/rel="canonical"/.test(html)) problem(rel, '404 page must not have a canonical');
      checkFooter(rel, html);
      continue;
    }
    canonicals.push(checkPage(rel, html));
  }

  if (!existsSync(join(OUT, 'fonts/OFL.txt'))) problem('fonts/OFL.txt', 'missing IBM Plex licence text');
  if (existsSync(join(OUT, '.htmlvalidate.json'))) problem('.htmlvalidate.json', 'validator config must not be deployed');

  // Nothing may load from Google Fonts: the Plex fonts are self-hosted.
  for (const f of files.filter((x) => /\.(html|css|js|mjs)$/.test(x))) {
    if (/fonts\.(googleapis|gstatic)\.com/.test(readFileSync(f, 'utf8'))) problem(relative(OUT, f), 'requests Google Fonts');
  }

  // Every @font-face file and relative JS import must exist.
  const css = readFileSync(join(OUT, 'css/panel.css'), 'utf8');
  for (const [, url] of css.matchAll(/url\("([^"]+)"\)/g)) {
    if (!existsSync(join(OUT, 'css', url))) problem('css/panel.css', `missing ${url}`);
  }
  for (const f of files.filter((x) => /\.(js|mjs)$/.test(x))) {
    for (const [, spec] of readFileSync(f, 'utf8').matchAll(/\bfrom\s+'(\.[^']+)'/g)) {
      if (!existsSync(join(dirname(f), spec))) problem(relative(OUT, f), `import ${spec} does not exist`);
    }
  }

  // The site's Panel tokens stay in step with the brand templates.
  const brand = readFileSync(join(ROOT, 'tools/brand/templates/panel.css'), 'utf8');
  for (const token of ['--panel', '--ink', '--muted']) {
    const re = new RegExp(`:root\\s*\\{[^}]*${token}:\\s*(#[0-9A-Fa-f]{3,6})`);
    const want = brand.match(re)?.[1];
    const got = css.match(re)?.[1];
    if (!want || want.toUpperCase() !== got?.toUpperCase()) problem('css/panel.css', `${token} is ${got}, brand has ${want}`);
  }

  // sitemap.xml lists exactly the canonical URLs.
  const sitemap = [...readFileSync(join(OUT, 'sitemap.xml'), 'utf8').matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1]).sort();
  const want = canonicals.filter(Boolean).sort();
  if (JSON.stringify(sitemap) !== JSON.stringify(want)) {
    problem('sitemap.xml', `URLs ${JSON.stringify(sitemap)} differ from canonicals ${JSON.stringify(want)}`);
  }
  const robots = readFileSync(join(OUT, 'robots.txt'), 'utf8');
  if (!robots.includes(`Sitemap: ${BASE}sitemap.xml`)) problem('robots.txt', 'does not point to the sitemap');

  if (problems.length) {
    console.error(`site check failed (${problems.length}):\n- ${problems.join('\n- ')}`);
    process.exit(1);
  }
  console.log(`site OK: ${htmlFiles.length} pages`);
}

main();
