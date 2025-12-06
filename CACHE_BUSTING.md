# Cache Busting for JavaScript and CSS

This project includes cache-busting functionality to prevent browsers from caching outdated JavaScript and CSS files.

## How It Works

The `index.html` file includes version query parameters for `app.js` and `style.css`:
- `<script src="app.js?v=1765057114"></script>`
- `<link rel="stylesheet" href="style.css?v=1765057114">`

When the version number changes, browsers will fetch the new files instead of using cached versions.

## Scripts

### `update-js-version.sh`

Manually updates the version number in `index.html` to bust the browser cache.

**Usage:**
```bash
./update-js-version.sh
```

This script:
- Generates a new timestamp-based version number
- Updates both `app.js` and `style.css` version parameters in `index.html`
- Saves the version to `web/.js-version` (gitignored)

### `watch-js-changes.sh`

Automatically watches `web/app.js` for changes and updates the version when the file is modified.

**Usage:**
```bash
./watch-js-changes.sh
```

**Requirements:**
- `fswatch` must be installed
  - macOS: `brew install fswatch`
  - Linux: `sudo apt-get install fswatch`

**Note:** This script runs continuously until you press Ctrl+C.

## When to Use

- **Manual update**: Run `./update-js-version.sh` after making changes to `app.js` or `style.css`
- **Automatic watching**: Run `./watch-js-changes.sh` in a separate terminal while developing to automatically update versions on file changes

## Browser Cache Clearing

If you still see cached content after updating the version:
1. Hard refresh: `Cmd+Shift+R` (macOS) or `Ctrl+Shift+R` (Windows/Linux)
2. Clear browser cache manually
3. Use incognito/private browsing mode for testing

