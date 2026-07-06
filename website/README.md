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

Install dependencies:

```bash
cd website
npm install
```

Run the local development server:

```bash
npm run dev
```

Astro prints the local URL, usually `http://localhost:4321`. The custom homepage is available at `/`, and the documentation site is available at `/docs/`.

## Static Build With Search

Generate the static site:

```bash
npm run build
```

The build writes static output to:

```text
website/dist/
```

Starlight builds the documentation search index during `npm run build` using Pagefind. The generated search assets are included in `website/dist/`, so the deployed static site has client-side documentation search without a separate search service.

Preview the generated static site locally:

```bash
npm run preview
```

`npm run preview` serves the already-built `dist/` output. Run `npm run build` again after content or configuration changes before previewing production output.

## Deployment

Deploy the contents of `website/dist/` to any static host after running:

```bash
cd website
npm install
npm run build
```

No server-side runtime is required for the website or docs search. The site is static HTML, CSS, JavaScript, images, and Pagefind search assets.

Example S3 upload:

```bash
cd website
npm install
npm run build
aws s3 sync dist/ s3://your-bucket-name/ --delete
```

Configure the bucket, or CloudFront distribution in front of it, to serve `index.html` as the index document. The generated site should be accessed over HTTP, for example through an S3 website endpoint or CloudFront URL. Opening `dist/index.html` directly with `file://` is not a supported preview path because routing, module scripts, assets, and search files expect an HTTP origin.

## Content Policy

Keep the website docs focused on user-facing setup, concepts, operations, and examples. The repository-level `docs/` directory remains the source for internal design notes and deeper implementation records. Copy or curate content into Starlight when it is useful for website readers rather than mirroring every internal document.
