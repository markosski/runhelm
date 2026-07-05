# Project Website

The `website/` directory contains the static RunHelm website and documentation site. It uses Astro with Starlight.

## Structure

```text
website/
  astro.config.mjs
  package.json
  public/
    runhelm-logo.png
  src/
    pages/
      index.astro
    content/
      docs/
        docs/
```

The custom homepage lives at `/`. Starlight documentation pages live under `/docs/` by placing content in `website/src/content/docs/docs/`.

## Development

```bash
cd website
npm install
npm run dev
```

Build and preview the static site:

```bash
npm run build
npm run preview
```

## Content Policy

Keep the website docs focused on user-facing setup, concepts, operations, and examples. The repository-level `docs/` directory remains the source for internal design notes and deeper implementation records. Copy or curate content into Starlight when it is useful for website readers rather than mirroring every internal document.
