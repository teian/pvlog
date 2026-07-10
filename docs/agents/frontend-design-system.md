# Agent: Frontend Design System

Use these rules for frontend typography, colour, styling, and runtime assets.

## Typography and Fonts

The application follows the Bundesregierung Styleguide typography guidance for digital applications while using the matching **Noto** font families as bundled, screen-optimised counterparts. Typography must stay readable, accessible, and locally bundled.

- **Primary font family**: Use `Noto Sans` as the default font family for digital UI text.
- **Headline usage**: Use `Noto Sans` for headings, subheadings, captions, and emphasis. Keep heading weights restrained and readable.
- **Secondary font family**: Use `Noto Serif` only when a deliberate serif editorial style is needed. Do not make it the default application font.
- **Condensed family**: Use `Noto Sans Condensed` only for image wordmarks and similar logo treatments. Do not use it for normal UI text.
- **Display weights**: Use heavier or lighter Noto weights for titles, wordmarks, or other large Schautexte only. Do not use extreme weights for body copy, labels, or long-form text.
- **Webfonts only**: In digital applications, use screen-optimised webfont files. Do not use Office or DTP font files in the app.
- **Non-Latin scripts**: Use the matching script-specific Noto family, such as `Noto Sans Arabic`, `Noto Sans CJK`, or `Noto Serif CJK`, when the default Noto family does not cover the script. If the required Noto font is unavailable, fall back to installed system fonts.
- **Readability**: Choose font sizes and line heights that remain readable at the relevant viewing distance and content density. Verify typography choices against accessibility requirements.
- **Local assets only**: Font assets must be bundled locally in the repo or through package registry packages. Do not load fonts from external CDNs or remote `<link>` tags.
- **Centralised font stacks**: Define global font stacks or tokens in `src/index.css` and reuse them consistently. Avoid ad hoc `font-family` declarations in feature components or inline styles.

## Colour Scheme

The current frontend theme is implemented in `src/index.css` as a neutral OKLCH token system based on shadcn/ui conventions.

- **Theme definition**: `src/index.css` — CSS custom properties in `:root` for light mode and `.dark` for dark mode.
- **Tailwind integration**: Variables are registered in the `@theme inline` block as `--color-*`, enabling utilities like `bg-primary`, `text-destructive`, `border-border`, `text-muted-foreground`, and `bg-sidebar`.
- **Theme shape**: The palette is intentionally neutral, with semantic roles expressed through token names rather than brand colour names.
- **Raw colour format**: Theme tokens currently use OKLCH values. Component code must consume semantic tokens instead of raw OKLCH, hex, RGB, HSL, or named colours.

## Palette Reference

The implemented palette is a semantic OKLCH scale rather than a named brand palette.

| Token Group | Purpose | Light Theme | Dark Theme |
| --- | --- | --- | --- |
| `background` / `foreground` | App surface and default text | Subtle warm off-white surface with dark warm-neutral text | Near-black surface with near-white text |
| `card` / `card-foreground` | Card surfaces and text | Almost-white raised surface | Raised dark surface |
| `popover` / `popover-foreground` | Floating surfaces and text | Almost-white raised surface | Raised dark surface |
| `primary` / `primary-foreground` | Primary actions and emphasis | Dark warm-neutral with warm off-white text | Near-white with near-black text |
| `secondary` / `secondary-foreground` | Secondary surfaces and controls | Very light warm-neutral surface with dark text | Dark neutral with light text |
| `muted` / `muted-foreground` | Subtle backgrounds and secondary text | Soft warm-neutral surface with higher-contrast secondary text | Dark neutral with mid-gray text |
| `accent` / `accent-foreground` | Hover, selected, and low-emphasis interactive states | Soft cream hover state with dark text | Dark neutral with light text |
| `destructive` | Error and destructive actions | Saturated red OKLCH token | Lighter red OKLCH token |
| `border` / `input` / `ring` | Borders, form chrome, and focus rings | Soft warm-neutral borders and focus rings | Transparent light overlays and mid neutral ring |
| `chart-1` through `chart-5` | Chart series colours | Neutral ramp | Same neutral ramp |
| `sidebar-*` | Sidebar surfaces, text, emphasis, borders, and rings | Light neutral sidebar | Dark neutral sidebar with blue-violet sidebar primary |
| `master-data-*` | Nested master-data navigation emphasis | Pale warm active item and orange count badge | Dark warm active item and orange count badge |

## Semantic Mapping

| CSS Variable | Light Theme | Dark Theme | Tailwind Utility |
| --- | --- | --- | --- |
| `--background` | `oklch(0.988 0.006 86)` | `oklch(0.145 0 0)` | `bg-background` |
| `--foreground` | `oklch(0.155 0.01 72)` | `oklch(0.985 0 0)` | `text-foreground` |
| `--primary` | `oklch(0.22 0.014 72)` | `oklch(0.922 0 0)` | `bg-primary`, `text-primary` |
| `--primary-foreground` | `oklch(0.99 0.004 86)` | `oklch(0.205 0 0)` | `text-primary-foreground` |
| `--secondary` | `oklch(0.966 0.007 84)` | `oklch(0.269 0 0)` | `bg-secondary` |
| `--muted` | `oklch(0.959 0.008 84)` | `oklch(0.269 0 0)` | `bg-muted` |
| `--muted-foreground` | `oklch(0.45 0.014 72)` | `oklch(0.708 0 0)` | `text-muted-foreground` |
| `--accent` | `oklch(0.955 0.01 84)` | `oklch(0.269 0 0)` | `bg-accent` |
| `--destructive` | `oklch(0.577 0.245 27.325)` | `oklch(0.704 0.191 22.216)` | `bg-destructive`, `text-destructive` |
| `--border` | `oklch(0.882 0.008 82)` | `oklch(1 0 0 / 10%)` | `border-border` |
| `--input` | `oklch(0.86 0.01 82)` | `oklch(1 0 0 / 15%)` | `border-input` |
| `--ring` | `oklch(0.58 0.018 72)` | `oklch(0.556 0 0)` | `ring-ring` |
| `--sidebar` | `oklch(0.982 0.006 86)` | `oklch(0.205 0 0)` | `bg-sidebar` |
| `--sidebar-primary` | `oklch(0.22 0.014 72)` | `oklch(0.488 0.243 264.376)` | `bg-sidebar-primary` |
| `--master-data-active` | `oklch(0.91 0.055 78)` | `oklch(0.28 0.08 62)` | `bg-master-data-active` |
| `--master-data-badge` | `oklch(0.68 0.19 45)` | `oklch(0.68 0.19 45)` | `bg-master-data-badge` |

## Dark Theme Strategy

- Dark mode is activated through the `.dark` class and Tailwind's custom dark variant: `@custom-variant dark (&:is(.dark *));`.
- Dark surfaces invert the neutral scale: app backgrounds become near-black, text becomes near-white, and raised surfaces use a slightly lighter dark neutral.
- Primary actions invert from dark-on-light to light-on-dark so primary buttons remain high contrast.
- Secondary, muted, and accent surfaces share the same dark neutral token to keep low-emphasis UI quiet and consistent.
- Borders and inputs use translucent light overlays in dark mode, avoiding heavy outlines while preserving visible structure.
- The destructive token becomes lighter in dark mode to preserve contrast against dark surfaces.
- Sidebar tokens are defined separately from app tokens so navigation can tune its emphasis independently.

## Coding Rules

- **Always use design tokens**: Reference colours exclusively via CSS variables (`var(--color-*)`) or Tailwind utilities (`bg-primary`, `text-destructive`, `border-border`, `bg-muted`, etc.). Never hardcode OKLCH, hex, RGB, HSL, or named CSS colours in component styles or inline styles.
- **No arbitrary colour values**: Do not use Tailwind arbitrary values for colours, such as `bg-[oklch(0.205_0_0)]` or `bg-[#111111]`. Use the semantic token instead, such as `bg-primary`.
- **Semantic intent over visual description**: Choose the token that matches the meaning, not the visual appearance. Use `destructive` for errors or destructive actions, `primary` for primary emphasis, `muted` for low-emphasis surfaces, and `accent` for hover or selected states.
- **New colours**: If a new semantic colour is needed, first check whether an existing token fits. If not, add a named semantic token to `src/index.css` in both `:root` and `.dark`, register it in the `@theme inline` block, and document it here.
- **Contrast verification**: When combining foreground/background colours, verify WCAG AA contrast: 4.5 : 1 for normal text, 3 : 1 for large text / UI components, against both light and dark themes. Use browser DevTools or another contrast checker that supports OKLCH-derived colours.
- **`index.css` is the only file** that may contain raw hex colour values for defining theme tokens. All other files must use tokens.
- **No `@apply`**: Tailwind `@apply` is banned in CSS files.

## Stylelint Enforcement

- `at-rule-disallowed-list: ["apply"]` — bans `@apply` in all CSS files.
- `color-named: "never"` — bans named CSS colours, such as `red`, `blue`, `white`.
- `function-disallowed-list: ["rgb", "rgba", "hsl", "hsla", "hwb", "lch", "oklch", "lab", "oklab"]` — bans hardcoded colour functions and forces use of `var(--color-*)` tokens instead. The theme definition in `src/index.css` is the sole exception.

## Dependencies and Assets

- **No External Dependencies**: All libraries must be in `package.json` and resolved from `node_modules`. No CDN or remote registry at runtime.
- **Assets**: Vendor fonts, icons, and images into the repo or as package registry packages. No external URLs for assets.
- **CSP Compliance**: No `<script>`, `<link>`, or `<img>` referencing external origins.
