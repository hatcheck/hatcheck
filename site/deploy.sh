#!/bin/sh
# Ships the landing page: wrangler deploy from this folder, which uploads index.html and the
# logo files as Cloudflare static assets and keeps hatcheck.dev attached (see wrangler.jsonc).
# Needs wrangler signed in to the Cloudflare account that holds the hatcheck.dev zone.
cd "$(dirname "$0")" || exit 1
if command -v wrangler >/dev/null 2>&1; then
    exec wrangler deploy "$@"
fi
exec npx wrangler deploy "$@"
