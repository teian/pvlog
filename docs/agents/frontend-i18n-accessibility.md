# Agent: Frontend i18n and Accessibility

Use these rules for frontend internationalization and accessibility.

## Internationalization (i18n)

- **Library**: `i18next` + `react-i18next` with `i18next-browser-languagedetector`.
- **Supported Languages**: English (`en`) and German (`de`). English is the fallback language.
- **Language Detection**: The browser's preferred language is detected automatically via the `navigator` API. The user's choice is persisted in `localStorage` under `faultmanagement-language`.
- **Translation Files**: JSON files located at `src/shared/lib/i18n/locales/{en,de}.json`. Each language has a single flat namespace, `translation`.
- **Initialization**: `src/shared/lib/i18n/i18n.ts` is imported as a side effect in `src/main.tsx` before the app renders.
- **No Hardcoded User-Facing Text**: Every user-visible string in JSX must use the `t()` function from `useTranslation()`. This includes:
  - Element children, such as headings, paragraphs, buttons, labels, badges, and menu items.
  - Accessibility attributes, such as `aria-label` and `sr-only` spans.
  - Placeholder text on inputs and selects.
  - Tooltip content.
- **Allowed Exceptions**:
  - CSS class names and HTML attributes that are not user-visible, such as `id`, `htmlFor`, and `value` on select items.
  - Avatar fallback initials and other proper-noun abbreviations.
  - `src/shared/components/ui/` — mechanical shadcn/Radix wrappers do not contain translatable text.
- **Translation Key Convention**: Use dot-separated, camelCase keys structured by domain: `nav.*`, `pages.<pageName>.*`, `sidebar.*`, `theme.*`, `showcase.<section>.*`, `header.*`, `features.<featureName>.*` for feature-scoped strings, and `shared.<componentName>.*` for reusable shared component strings.
- **Adding New Text**: When adding any new user-visible text, always:
  1. Add the key + English value to `src/shared/lib/i18n/locales/en.json`.
  2. Add the key + German value to `src/shared/lib/i18n/locales/de.json`.
  3. Use `t('key')` in the component via the `useTranslation` hook.
- **ESLint Enforcement**: The rule `react/jsx-no-literals` warns on hardcoded string literals in JSX children. Fix violations by extracting the string to the translation files and using `t()`.
- **Navigation Data**: Navigation labels are stored as i18n keys, `labelKey`, in `src/widgets/Layout/navigationData.ts` and resolved with `t()` at render time.

## Accessibility (a11y)

The application must conform to **WCAG 2.1 Level AA**, the standard mandated by **EN 301 549** in the EU and **BITV 2.0** in Germany. All new and modified code must satisfy the rules below.

### Legal Context

- **EN 301 549 v3.2.1** — EU harmonised standard for ICT accessibility, references WCAG 2.1 AA.
- **BITV 2.0** — German federal regulation, Barrierefreie-Informationstechnik-Verordnung, requires WCAG 2.1 AA for public-sector web applications. Private-sector digital products fall under the **Barrierefreiheitsstärkungsgesetz (BFSG)**, effective June 2025.
- **European Accessibility Act (EAA)** — Directive 2019/882, transposed into German law via BFSG.

### ESLint Enforcement

`eslint-plugin-jsx-a11y` is configured with the strict preset in `eslint.config.js`. Key rules enforced as errors:

- `jsx-a11y/alt-text` — every `<img>`, `<area>`, `<input type="image">`, and `<object>` must have `alt` text.
- `jsx-a11y/anchor-has-content` — anchors must have accessible content.
- `jsx-a11y/aria-props` / `jsx-a11y/aria-proptypes` / `jsx-a11y/aria-unsupported-elements` — validate ARIA attribute names and values.
- `jsx-a11y/click-events-have-key-events` + `jsx-a11y/no-static-element-interactions` — interactive handlers on non-interactive elements require keyboard support.
- `jsx-a11y/heading-has-content` — headings must have accessible content. This is disabled for `src/shared/components/ui/` where content is passed via spread props.
- `jsx-a11y/label-has-associated-control` — every `<label>` must be linked to a form control via `htmlFor` or nesting with `assert: 'either'`.
- `jsx-a11y/no-noninteractive-element-interactions` — non-interactive elements must not have interactive handlers.
- `jsx-a11y/no-redundant-roles` — do not add implicit ARIA roles.

### Coding Rules

- **Semantic HTML first**: Use `<nav>`, `<main>`, `<header>`, `<footer>`, `<section>`, `<article>`, `<aside>`, `<button>`, and `<a>` instead of `<div>` with `role` or click handlers.
- **Landmarks**: Every page must be wrapped in a `<main>` landmark. The layout already provides `<header>` and `<nav>`.
- **Headings**: Maintain a logical heading hierarchy, `h1` -> `h2` -> `h3`. Each page must have exactly one `<h1>`.
- **Icon-only buttons**: Must have an `aria-label` translated via `t()` or a `<span className="sr-only">` child.
- **Icon-only links**: Must have an `aria-label` translated via `t()`. This is critical for collapsed sidebar navigation.
- **Images**: All `<img>` elements must have an `alt` attribute. Decorative images use `alt=""`.
- **Form controls**: Every input must be associated with a `<Label>` via `htmlFor`/`id` or wrapping. Placeholder text is not a substitute for a label.
- **Focus management**:
  - All interactive elements must be keyboard-reachable and operable.
  - Visible focus indicators are required, such as `focus-visible:ring-*` classes in Tailwind.
  - Do not set `tabIndex` > 0.
  - After modal/dialog open, focus must move into the dialog. Radix handles this.
  - After modal/dialog close, focus must return to the trigger element. Radix handles this.
- **Color contrast**: Text and interactive elements must meet WCAG AA contrast ratios: 4.5:1 for normal text, 3:1 for large text / UI components. Verify against both light and dark theme tokens.
- **`lang` attribute**: The `<html lang>` attribute is synced with i18next at runtime in `src/shared/lib/i18n/i18n.ts`. Screen readers depend on this for correct pronunciation.
- **Accessible names for screen-reader text**: All `aria-label`, `aria-labelledby`, and `sr-only` text must be translated via `t()` so German screen reader users get localised labels.
- **Reduced motion**: Respect `prefers-reduced-motion`. Animations should be disabled or simplified for users who opt out. Use the Tailwind `motion-safe:` / `motion-reduce:` variants.
- **No autoplaying media**: Audio or video must not auto-play with sound.
- **Error messages**: Form validation errors must be programmatically associated with the invalid control, using `aria-describedby` or `aria-errormessage`.
- **Skip links**: Consider adding a "Skip to main content" link as the first focusable element for keyboard users. This is recommended for BITV 2.0.

### Testing

- **Automated**: `eslint-plugin-jsx-a11y` catches about 30% of WCAG issues at lint time.
- **Semi-automated**: Use browser DevTools, Lighthouse, or axe DevTools for runtime audits. Target 0 violations.
- **Manual**: Keyboard-only navigation testing and screen reader verification with NVDA or VoiceOver before major releases.
- **E2E**: Consider adding axe-core integration to Playwright tests, `@axe-core/playwright`, for automated accessibility regression testing.
