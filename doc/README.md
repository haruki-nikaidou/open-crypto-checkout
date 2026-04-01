# Open Crypto Checkout — Documentation Site

The documentation site for [Open Crypto Checkout](https://github.com/haruki-nikaidou/open-crypto-checkout), built with [Astro](https://astro.build) and [Starlight](https://starlight.astro.build).

## Project Structure

```
doc/
├── public/              # Static assets (favicon, etc.)
├── src/
│   ├── assets/          # Images referenced in docs
│   └── content/
│       └── docs/        # Markdown / Markdoc pages
│           ├── quick-start/
│           │   ├── introduction.md
│           │   └── docker.md
│           ├── guides/
│           │   ├── configuration.md
│           │   ├── deploy-systemd.md
│           │   ├── frontend.md
│           │   └── webhooks.md
│           └── reference/
│               ├── authentication.md
│               ├── service-api.md
│               ├── user-api.md
│               └── admin-api.md
├── astro.config.mjs
├── markdoc.config.mjs
├── package.json
└── tsconfig.json
```

Pages are `.md` or `.mdx` files inside `src/content/docs/`. Each file is automatically exposed as a route based on its path.

## Commands

Run from this directory (`doc/`):

| Command | Action |
| :--- | :--- |
| `pnpm install` | Install dependencies |
| `pnpm dev` | Start local dev server at `localhost:4321` |
| `pnpm build` | Build the production site to `./dist/` |
| `pnpm preview` | Preview the production build locally |

## Adding Documentation

Add a new `.md` or `.mdx` file under `src/content/docs/` and register it in the sidebar inside `astro.config.mjs`:

```js
{ label: 'My New Page', slug: 'guides/my-new-page' }
```

Images go in `src/assets/` and can be referenced with relative paths in Markdown.

## Resources

- [Starlight documentation](https://starlight.astro.build/)
- [Astro documentation](https://docs.astro.build)
- [Markdoc integration](https://docs.astro.build/en/guides/integrations-guide/markdoc/)
