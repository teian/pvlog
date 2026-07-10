# Agent: Frontend Aesthetics

Canonical reference derived from `DashboardPage` ("Meine Aufgaben" — `src/pages/DashboardPage.tsx`). Apply when generating or reviewing any frontend UI.

## Typography

Fonts are **Noto Sans** (body/UI) and **Noto Serif** (editorial), defined in `src/index.css`. Full font rules in [frontend-design-system.md](frontend-design-system.md).

**Scale in use:**

| Class | Use |
|---|---|
| `text-[10px] font-semibold uppercase tracking-widest` | Sidebar section group labels, nav dividers |
| `text-xs font-semibold uppercase tracking-widest` | Column headers, drawer section titles, badge labels |
| `text-xs` | Meta, secondary info, badges |
| `text-sm` | Default body, row content, form fields |
| `text-2xl font-bold tracking-tight` | Page `<h1>` |
| `text-4xl font-bold tabular-nums` | KPI value display |

**Modifiers — always apply:**
- `font-mono` — claim numbers, dates, keyboard hints, any code-like string
- `tabular-nums` — any number that may reflow (counts, stats)
- `uppercase tracking-widest` — section dividers and table column headers
- `leading-snug` — multi-line cells and drawer headings
- `truncate` — any text in a fixed-width column or sidebar label

## Color & theme

All colors use semantic tokens. Raw hex/OKLCH forbidden outside `src/index.css`. Full palette in [frontend-design-system.md](frontend-design-system.md).

**Brand accent:** `orange-500` — logo mark only. Do not use on interactive elements or data.

**Status palette (contextual, not global):**

| State | Surface | Text / Dot |
|---|---|---|
| Overdue / urgent | `border-destructive/40 bg-destructive/5` | `text-destructive` |
| Warning / due soon | — | `text-amber-500 dark:text-amber-400` |
| Containment action badge | `border-amber-300 bg-amber-50 dark:bg-amber-950/30 dark:border-amber-800` | `text-amber-700 dark:text-amber-400` |
| Corrective / open badge | `border-blue-200 bg-blue-50 dark:bg-blue-950/30 dark:border-blue-800` | `text-blue-700 dark:text-blue-400` |
| Positive trend / completed | — | `text-emerald-600 dark:text-emerald-400` |

**Sidebar:** `bg-neutral-900` (outside token system); nav items `text-neutral-400`; active `bg-neutral-800 text-white`; hover `hover:text-neutral-200 hover:bg-neutral-800/60`.

**Avatar colors:** deterministic cycle by initials — `bg-emerald-600`, `bg-blue-600`, `bg-violet-600`, `bg-amber-600`, `bg-rose-600`.

**Inline assignee avatar (content area):** `bg-primary/15 text-primary` — uses tokens, not raw colors.

## Layout

- **Sidebar:** fixed `w-60 bg-neutral-900`, `z-30`
- **Content area:** `ml-60 flex-1`, inner max-width `max-w-screen-xl mx-auto px-6 py-6`
- **Page stack:** `flex flex-col gap-6` — consistent vertical rhythm between all major sections
- **KPI grid:** `grid grid-cols-2 sm:grid-cols-4 gap-4`
- **Data table rows:** explicit `gridTemplateColumns` string (e.g. `140px 1fr 200px 120px 140px 120px`) — never flexbox for row alignment
- **Filter bar:** `flex items-center gap-2 flex-wrap`
- **Card padding:** `p-5` for KPI cards, `p-4` for content cards, `px-4 py-3` for table rows

## Backgrounds

This is a dense data application — no decorative gradients or geometric patterns.

- App: `bg-background` token
- Card surfaces: `bg-card`
- Table header rows / muted strips: `bg-muted/30`
- Urgent KPI: `bg-destructive/5` with `border-destructive/40`
- Drawer overlay: `bg-black/25 backdrop-blur-sm`
- Sidebar: `bg-neutral-900` (intentionally off-token for strong visual separation)

## Motion

- `transition-colors` on every interactive element — links, buttons, nav items, badges
- `animate-pulse` for skeleton loaders — full-card `bg-muted border-0` blocks; text lines use `h-3.5 bg-muted animate-pulse rounded-md`
- Side drawer uses CSS class `slide-in-right` (defined in global CSS, not a Tailwind utility)
- `backdrop-blur-sm` on drawer overlay
- No entrance animations on data rows or KPI cards

## Components

Always use shadcn/ui primitives: `Card`, `Button`, `Badge`, `Alert`, `Dialog`. Never hand-roll these.

**KPI card:**
- `Card` with `p-5 flex flex-col gap-3`
- Icon top-right `text-muted-foreground` (or `text-destructive` if urgent)
- Label: `text-xs font-semibold uppercase tracking-widest text-muted-foreground`
- Value: `text-4xl font-bold tabular-nums` (+ `text-destructive` if urgent)
- Trend inline before sub-label: `font-medium mr-1` colored by `text-emerald-600`/`text-destructive`/`text-muted-foreground`

**Underline tabs:**
- Wrapper: `border-b border-border`
- Each tab: `border-b-2 -mb-px px-4 py-2.5 text-sm font-medium transition-colors`
- Active: `border-primary text-foreground`
- Inactive: `border-transparent text-muted-foreground hover:text-foreground hover:border-border`
- Count badge: `text-xs rounded-full px-1.5 py-0.5 font-mono`; active `bg-primary/10 text-primary`, inactive `bg-muted text-muted-foreground`

**Segmented control:**
- `flex rounded-md border border-border overflow-hidden` — no gap between items
- `border-l border-border` separates adjacent items
- Active: `bg-foreground text-background`; inactive: `text-muted-foreground hover:text-foreground hover:bg-muted/50`

**Toggle filter button:**
- `aria-pressed` required
- Active: `border-primary bg-primary/10 text-primary`
- Inactive: `border-border text-muted-foreground hover:text-foreground`

**Side drawer:**
- `fixed right-0 top-0 h-full w-[760px] max-w-full bg-background border-l border-border z-50 flex flex-col shadow-2xl`
- Backdrop: `fixed inset-0 bg-black/25 backdrop-blur-sm z-40`
- Scroll region: `flex-1 min-h-0 overflow-y-scroll`
- Section heading inside drawer: `text-xs font-semibold uppercase tracking-widest text-muted-foreground mb-3`

**Table-style card:**
- `Card` with `overflow-hidden`
- Header row: `bg-muted/30 text-xs font-semibold text-muted-foreground uppercase tracking-widest px-4 py-2.5`
- Data rows: `border-b border-border/40 transition-colors hover:bg-accent/40`; active row `bg-accent/60`

**Assignee cell:** `size-6 rounded-full bg-primary/15`, initials `text-[10px] font-bold text-primary`

**Claim tag:** `Badge variant="outline"` with `font-mono text-xs`
