# mPDF

mPDF is a PDF editor for Windows that works without any online service. It is a fork of
[PdfCraft](https://github.com/storytold/pdfcraft) by the ArtCraft team, used under its
`MIT OR Apache-2.0` licence. mPDF is not an ArtCraft product.

## What differs from PdfCraft

- No update check and no self-update. A new version is installed by an administrator.
- No links to a publisher's website, chat or source page.
- The interface starts in German. Every other language is still in Preferences.
- Added images can be placed behind the page's text, and the page's right-click menu has
  **Add image behind the text** and **Add background…**.
- Its own name and icon. The installer takes PdfCraft's place: installing mPDF replaces an
  installed PdfCraft, they do not sit side by side.

A document's own web links still open in the browser, after a confirmation.

## Build

The Windows installer (MSI) and the portable zip are built by
[`.github/workflows/build-windows.yml`](.github/workflows/build-windows.yml), which runs
`packaging/windows/package.ps1`. Each build of `main` is published as a pre-release. The files
are not signed.

For a local build and the tests, see the commands in [`AGENTS.md`](AGENTS.md).

## Licence

Code: `MIT OR Apache-2.0` ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)), with the
notices in [NOTICE](NOTICE). The Merkel signet and the mPDF app icon are trademarks and are not
open source: [docs/brand/LICENSE-brand.txt](docs/brand/LICENSE-brand.txt). Third-party assets are
listed in [ATTRIBUTION.md](ATTRIBUTION.md).
