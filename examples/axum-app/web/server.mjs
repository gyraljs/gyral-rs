import { html, raw } from '@gyral/core';
import { renderToString } from '@gyral/ssr';
import { VisitStore } from './components.mjs';
import { documentCss } from './styles.mjs';

// The SSR style hasher needs encode(). This is a UTF-8 subset, not the full Web API.
globalThis.TextEncoder ??= class {
  encode(input = '') {
    const bytes = [];
    for (const char of String(input)) {
      const raw = char.codePointAt(0);
      const cp = raw >= 0xd800 && raw <= 0xdfff ? 0xfffd : raw;
      if (cp <= 0x7f) bytes.push(cp);
      else if (cp <= 0x7ff) bytes.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
      else if (cp <= 0xffff) bytes.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
      else bytes.push(0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 0x3f), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
    }
    return new Uint8Array(bytes);
  }
};

globalThis.gyralBundleVersion = 1;
globalThis.gyralRender = async ({ page, props }) => {
  if (page === 'broken') return renderToString(html`<gyral-broken-card></gyral-broken-card>`, { onError: 'throw' });
  if (page !== 'home') throw new Error('Unknown example page');
  const visit = VisitStore.instance({ name: props.name, count: props.visits ?? 7 });
  try {
  return await renderToString(html`<!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width,initial-scale=1">
        <title>Gyral rendered inside Rust</title>
        ${raw(`<style>${documentCss}</style>`)}
        <script type="module" src=${__GYRAL_CLIENT_PATH__} integrity=${__GYRAL_CLIENT_INTEGRITY__} crossorigin="anonymous"></script>
      </head>
      <body>
        <main>
          <gyral-greeting .name=${props.name}></gyral-greeting>
          <p>This page was rendered by Gyral inside the Rust process.</p>
          <section aria-labelledby="counter-title">
            <h2 id="counter-title">Local counter</h2>
            <gyral-counter></gyral-counter>
          </section>
          <gyral-stores .instances=${[visit]}>
            <section aria-labelledby="store-title">
              <h2 id="store-title">Shared visits</h2>
              <gyral-store-card></gyral-store-card>
              <gyral-store-card></gyral-store-card>
            </section>
            <section aria-labelledby="form-title">
              <h2 id="form-title">Change your name</h2>
              <gyral-name-form .name=${props.name}></gyral-name-form>
            </section>
          </gyral-stores>
          <gyral-time-card .instant=${props.instant ?? '2026-11-01T06:30:00Z'} .zone=${props.zone ?? 'America/New_York'}></gyral-time-card>
        </main>
      </body>
    </html>`, { onError: 'throw' });
  } finally { visit.dispose(); }
};
