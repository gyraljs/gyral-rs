import { command, define, html, prop, subscription } from '@gyral/core';

define()('gyral-client-greeting', {
  props: { name: prop.string({ default: 'friend' }) },
  view: (_state, _intent, { props }) => html`<p>Hello, <bdi>${props.name}</bdi>. This greeting was rendered in your browser.</p>`,
  styles: '@layer base { :host { display: block; } p { line-height: 1.6; overflow-wrap: anywhere; } }',
});

// A Gyral driver observes native controls. It never owns their values or submits.
const drafts = subscription('native-note-draft', (emit, { input: formId, fail }) => {
  const form = document.getElementById(formId);
  if (!(form instanceof HTMLFormElement)) {
    fail(new Error('The note preview needs its native form.'));
    return () => {};
  }
  const title = form.elements.namedItem('title');
  const body = form.elements.namedItem('body');
  if (!(title instanceof HTMLInputElement) || !(body instanceof HTMLTextAreaElement)) {
    fail(new Error('The note preview needs title and body controls.'));
    return () => {};
  }
  const read = () => emit({ title: title.value, body: body.value });
  form.addEventListener('input', read);
  // Read the actual inputs on upgrade, including anything typed before JS loaded.
  read();
  return () => form.removeEventListener('input', read);
});

define()('gyral-note-preview', {
  props: {
    formId: prop.string({ default: 'note-editor' }),
    initialTitle: prop.string({ default: '' }),
    initialBody: prop.string({ default: '' }),
  },
  init: props => ({ title: props.initialTitle, body: props.initialBody }),
  intent: { DraftChanged: true },
  update: { DraftChanged: (_state, msg) => msg.draft },
  subscriptions: (_state, props) => [command(drafts, props.formId, {
    key: 'note-draft', onSuccess: draft => ({ _tag: 'DraftChanged', draft }),
  })],
  view: state => html`
    <h3 dir="auto">${state.title.trim() || 'Untitled note'}</h3>
    <div class="body" dir="auto">${state.body || 'Your note will appear here as you type.'}</div>
    <p class="count"><output aria-label="Note character count">${[...state.body.replaceAll('\r\n', '\n')].length}</output> / 4,000 characters</p>
  `,
  styles: `
    @layer base {
      :host { display: block; min-inline-size: 0; }
      h3 { margin: 1rem 0; overflow-wrap: anywhere; }
      .body { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.6; min-block-size: 9rem; }
      .count { border-block-start: 1px solid var(--border); padding-block-start: 1rem; color: var(--muted); }
    }
  `,
});

define()('gyral-client-counter', {
  props: { start: prop.number({ default: 5 }) },
  init: props => ({ count: props.start }),
  intent: { Increment: true, Reset: true },
  update: {
    Increment: state => ({ count: Math.min(999, state.count + 1) }),
    Reset: (_state, _msg, { props }) => ({ count: props.start }),
  },
  view: (state, intent) => html`
    <p class="count">Count <output aria-live="polite">${state.count}</output></p>
    <div class="controls">
      <button type="button" data-intent=${intent.Increment} ?disabled=${state.count >= 999}>Increment</button>
      <button class="secondary" type="button" data-intent=${intent.Reset}>Reset</button>
    </div>
  `,
  styles: `
    @layer reset, base, components;
    @layer reset { *, *::before, *::after { box-sizing: border-box; } button { font: inherit; } }
    @layer base {
      :host { display: block; min-block-size: 11rem; padding-block: 1rem; }
      :focus-visible { outline: 3px solid var(--accent); outline-offset: 4px; }
    }
    @layer components {
      .count { display: grid; gap: .25rem; margin: 0 0 1.5rem; }
      output { font-size: clamp(2.5rem, 6vw, 4rem); font-weight: 750; line-height: 1.1; font-variant-numeric: tabular-nums; }
      .controls { display: flex; flex-wrap: wrap; gap: .75rem; }
      button { min-block-size: 44px; min-inline-size: 44px; padding: .6rem 1rem; border: 1px solid var(--accent); border-radius: .5rem; color: var(--on-accent); background: var(--accent); cursor: pointer; }
      .secondary { background: var(--surface); color: var(--text); border-color: var(--border); }
      button:disabled { cursor: default; opacity: .6; }
    }
  `,
});
