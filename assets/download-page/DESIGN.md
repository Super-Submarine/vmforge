# VMForge wave-1 download page — visual design (annotated)

**Author:** Remy (Product Designer) · **Contract:** 019f8aba-85cc-7c58-b18f-e25f7cf4a05e

`index.html` in this directory is a self-contained HTML mock built entirely from
Visual Design System v1 tokens (company doc `019f8a53-2ce6-7c9a-9587-e6a7f250b962`);
it is the normative reference for the wave-1 download/landing page section. Open it
in a browser — it supports light and dark (`prefers-color-scheme`) with token swaps only.

Screenshots: [light](screenshot-light.png) · [dark](screenshot-dark.png)

## Annotations

### Hero
- App icon 96 px (render from `../icons/`, never a screenshot of it), centered.
- H1 "Snapshot your VMs like code." — leads with the snapshot-tree wedge, not category
  boilerplate. Sub-line names the three USP hypotheses in one sentence, flagged as beta.
- Exactly two CTAs: `.deb` primary (`color.accent.emphasis` fill, h 40) and `.AppImage`
  secondary (1 px `border.strong`) — mirrors DS v1 button hierarchy; no third CTA.
- Meta line under CTAs (`type.caption`, `fg.secondary`): version · arch · "GPG-signed" ·
  SHA256SUMS — trust signals visible before scroll.

### Install section
- Two equal cards (`bg.surface`, `radius.lg`, 1 px `border.default`), one per artifact;
  collapses to one column below 720 px.
- Each card: title + distro badge, one-line "who this is for", then a numbered 3-step
  list with copy-pasteable commands in `font.family.mono` on `bg.inset` wells.
- Step 2 in both cards is **Verify** and links to the callout — verification is part of
  the golden path, not an appendix.

### GPG verification callout
- Distinct treatment: `bg.surface` card with 4 px `accent.fg` left border — informational
  emphasis, not a warning (verification is routine, not scary).
- One copy-pasteable block: import key → `gpg --verify SHA256SUMS.asc` → `sha256sum -c`.
  Matches the release pipeline artifacts (PR #18: `.deb`, `.AppImage`, `SHA256SUMS` +
  detached `.asc`).
- Fingerprint line is an explicit placeholder until `VMFORGE_RELEASE_GPG_PRIVATE_KEY`
  is provisioned by IT Security; **must** be replaced before launch — fail-closed copy
  ("If verification fails, do not install").

### Tokens used (DS v1)
`color.bg.canvas/surface/inset`, `color.fg.primary/secondary/on-accent`,
`color.accent.fg/emphasis`, `color.border.default/strong`, `radius.md/lg`,
`font.family.ui` (Inter), `font.family.mono` (JetBrains Mono), base 14 px scale.
All text pairs inherit DS v1 §3.3 WCAG 2.1 AA ratios
(https://www.w3.org/TR/WCAG21/#dfn-contrast-ratio).

## Implementation notes
- The mock references icon renders relatively (`../icons/png/vmforge-256.png`); when
  lifted onto the site, inline or copy the assets.
- Download URLs use the `releases/latest/download/` pattern; adjust artifact filenames
  to whatever the release pipeline finally tags.
- Footer restates local-first privacy — one line, no marketing paragraph.
