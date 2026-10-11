import { define, defineForm, defineStore, form, html, prop, send } from '@gyral/core';
import { controlsCss } from './styles.mjs';

export const Greeting = define()('gyral-greeting', {
  props: { name: prop.string({ default: 'friend' }) },
  view: (_state, _intent, { props }) => html`<h1>Hello, ${props.name}</h1>`,
});

export const Counter = define()('gyral-counter', {
  styles: controlsCss,
  init: () => ({ count: 0 }),
  intent: { Increment: true },
  update: { Increment: (state) => ({ count: state.count + 1 }) },
  view: (state, intent) => html`
    <p>Count: <output>${state.count}</output></p>
    <button type="button" data-intent=${intent.Increment}>Increment</button>
  `,
});

export const VisitStore = defineStore('example-visit', {
  init: () => ({ name: 'unseeded', count: 0 }),
  update: {
    Increment: state => ({ ...state, count: state.count + 1 }),
    Rename: (state, msg) => ({ ...state, name: msg.name }),
  },
});

export const StoreCard = define()('gyral-store-card', {
  styles: controlsCss,
  stores: [VisitStore],
  init: () => ({}),
  intent: { AddVisit: true },
  update: { AddVisit: state => [state, [send(VisitStore, { _tag: 'Increment' })]] },
  view: (_state, intent, { read }) => html`
    <p>Shared name: <strong>${read(VisitStore).name}</strong></p>
    <p>Visits: <output>${read(VisitStore).count}</output></p>
    <button type="button" data-intent=${intent.AddVisit}>Add visit</button>
  `,
});

// A synchronous Standard Schema keeps validation independent of rendering.
const nameSchema = {
  '~standard': {
    version: 1,
    vendor: 'gyral-rs-example',
    validate: raw => {
      const name = typeof raw?.name === 'string' ? raw.name.trim() : '';
      const size = Array.from(name).length;
      return size >= 2 && size <= 64
        ? { value: { name } }
        : { issues: [{ message: 'Use 2–64 characters for your name.', path: ['name'] }] };
    },
  },
};
export const NameDefinition = defineForm(nameSchema, { secret: ['private_note'] });

export const NameForm = define()('gyral-name-form', {
  props: { name: prop.string({ default: 'friend' }) },
  styles: controlsCss,
  stores: [VisitStore],
  init: props => ({ _tag: 'Editing', name: props.name, enhancement: 'html' }),
  intent: { SaveName: form(NameDefinition, data => ({ _tag: 'SaveName', name: data.name })) },
  update: {
    Hydrated: state => ({ ...state, enhancement: 'interactive' }),
    SaveName: (state, msg) => [
      { _tag: 'Saved', name: msg.name, enhancement: state.enhancement },
      [send(VisitStore, { _tag: 'Rename', name: msg.name })],
    ],
    IntentRejected: (state, msg) => ({
      _tag: 'Rejected', name: typeof msg.values.name === 'string' ? msg.values.name : '',
      issues: msg.issues, values: msg.values, enhancement: state.enhancement,
    }),
  },
  view: (state, intent) => html`
    <form method="get" action="/" data-intent=${intent.SaveName}>
      <label for="name">Your name</label>
      <input id="name" name="name" autocomplete="name" value=${state.name}
        aria-describedby="name-help" aria-invalid=${state._tag === 'Rejected' ? 'true' : 'false'}>
      <p id="name-help">Use 2–64 characters. Saving changes this browser's shared name.</p>
      <label for="private-note">Synthetic private note</label>
      <input id="private-note" name="private_note" type="password" autocomplete="off" ?disabled=${state.enhancement === 'html'}>
      <p>The note is excluded from form error state. This example stores no data.</p>
      <button type="submit">Save name</button>
      ${state._tag === 'Rejected'
        ? html`<p role="alert">${state.issues.map(issue => issue.message).join(' ')}</p>`
        : state._tag === 'Saved' ? html`<p role="status">Saved for ${state.name}.</p>` : ''}
    </form>
  `,
});

export const TimeCard = define()('gyral-time-card', {
  props: {
    instant: prop.string({ default: '2026-11-01T06:30:00Z' }),
    zone: prop.string({ default: 'America/New_York' }),
  },
  init: props => ({ formatted: new Intl.DateTimeFormat('en-US', {
    timeZone: props.zone, month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit',
    timeZoneName: 'short',
  }).format(new Date(props.instant)) }),
  view: (state, _intent, { props }) => html`<p>Local time: <time datetime=${props.instant}>${state.formatted}</time></p>`,
});

// An explicit failing fixture checks the host's opaque-error path.
export const BrokenCard = define()('gyral-broken-card', {
  view: () => { throw new Error('synthetic-private-render-diagnostic'); },
});
