export const documentCss = `
@layer reset, tokens, base;
@layer reset {
  *, *::before, *::after { box-sizing: border-box; }
  body { margin: 0; }
}
@layer tokens {
  :root {
    color-scheme: light dark;
    --surface: light-dark(oklch(98% 0.01 250), oklch(20% 0.02 250));
    --text: light-dark(oklch(22% 0.03 250), oklch(95% 0.01 250));
    --accent: light-dark(oklch(40% 0.15 250), oklch(80% 0.10 250));
    --line: color-mix(in oklch, var(--text) 24%, var(--surface));
  }
}
@layer base {
  body { background: var(--surface); color: var(--text); font: 1rem/1.6 system-ui, sans-serif; overflow-wrap: anywhere; }
  main { max-inline-size: 72ch; margin-inline: auto; padding: clamp(1rem, 4vw, 3rem); }
  h1, h2 { line-height: 1.2; text-wrap: balance; }
  section { margin-block: 2rem; border-block-start: 1px solid var(--line); }
  p { overflow-wrap: anywhere; }
  :focus-visible { outline: 3px solid var(--accent); outline-offset: 3px; }
}
`;

export const controlsCss = `
@layer reset, controls;
@layer reset {
  *, *::before, *::after { box-sizing: border-box; }
  input, button { font: inherit; }
}
@layer controls {
  :host { display: block; }
  form { display: grid; grid-template-columns: minmax(0, 1fr); gap: 0.75rem; }
  input, button { min-block-size: 44px; border: 1px solid var(--line); border-radius: 0.4rem; padding: 0.5rem 0.75rem; }
  button { background: var(--accent); color: var(--surface); cursor: pointer; }
  input { min-inline-size: 0; inline-size: 100%; background: var(--surface); color: var(--text); }
  :focus-visible { outline: 3px solid var(--accent); outline-offset: 3px; }
  [role="alert"] { font-weight: 600; }
  p { overflow-wrap: anywhere; }
}
`;
