# VoidGate Design System

This document defines the visual language for VoidGate's user interface.

## Design Philosophy

VoidGate is a **zero-knowledge encrypted storage system**. The UI should reflect:

- **Privacy-first**: Dark theme default, minimal data exposure in UI
- **Trust and security**: Calm, professional aesthetic — not flashy or playful
- **Clarity**: Obvious locked/unlocked states, clear visual hierarchy
- **Minimalism**: Function over form, no unnecessary decoration

---

## Technology Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Framework | Leptos (Rust + WASM) | Single-language, Zero-Trace compliance |
| CSS Framework | Tailwind CSS | Utility-first, purges unused CSS, excellent dark mode |
| Build | Trunk | CSR bundler with Tailwind integration |

### Tailwind Configuration

```javascript
// tailwind.config.js
module.exports = {
  content: {
    files: ["*.html", "./src/**/*.rs"],
  },
  darkMode: 'class', // Enable class-based dark mode
  theme: {
    extend: {
      colors: {
        // Primary palette
        'void': {
          50:  '#f0f4f8',
          100: '#d9e2ec',
          200: '#bcccdc',
          300: '#9fb3c8',
          400: '#829ab1',
          500: '#627d98',
          600: '#486581',
          700: '#334e68',
          800: '#243b53',
          900: '#102a43',
          950: '#0a1929',
        },
        // Accent - muted teal (trust/security)
        'accent': {
          400: '#4fd1c5',
          500: '#38b2ac',
          600: '#319795',
          700: '#2c7a7b',
        },
        // Status colors
        'secure': '#10b981',   // Green - unlocked/success
        'locked': '#6b7280',   // Gray - locked state
        'warning': '#f59e0b',  // Amber - caution
        'danger': '#ef4444',   // Red - errors only
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Consolas', 'monospace'],
      },
    },
  },
  plugins: [],
}
```

---

## Color Palette

### Background Layers (Dark Theme)

| Layer | Color | Tailwind | Usage |
|-------|-------|----------|-------|
| Base | `#0a1929` | `bg-void-950` | App background |
| Surface | `#102a43` | `bg-void-900` | Cards, panels |
| Elevated | `#243b53` | `bg-void-800` | Modals, dropdowns |
| Hover | `#334e68` | `bg-void-700` | Interactive hover states |

### Text Colors

| Type | Color | Tailwind | Usage |
|------|-------|----------|-------|
| Primary | `#f0f4f8` | `text-void-50` | Headings, important text |
| Secondary | `#bcccdc` | `text-void-200` | Body text |
| Muted | `#829ab1` | `text-void-400` | Captions, hints |
| Disabled | `#627d98` | `text-void-500` | Disabled states |

### Accent & Status

| State | Color | Tailwind | Usage |
|-------|-------|----------|-------|
| Accent | `#38b2ac` | `text-accent-500` | Links, focus rings, primary actions |
| Success/Unlocked | `#10b981` | `text-secure` | Unlocked indicator, success messages |
| Locked | `#6b7280` | `text-locked` | Locked indicator |
| Warning | `#f59e0b` | `text-warning` | Session timeout warnings |
| Error | `#ef4444` | `text-danger` | Validation errors, failures |

---

## Typography

### Font Stack

- **UI Text**: Inter (clean, readable, modern)
- **Monospace**: JetBrains Mono (file names, paths, technical data)

### Scale

| Element | Size | Weight | Tailwind |
|---------|------|--------|----------|
| Page title | 24px | 600 | `text-2xl font-semibold` |
| Section heading | 18px | 600 | `text-lg font-semibold` |
| Body | 14px | 400 | `text-sm font-normal` |
| Caption | 12px | 400 | `text-xs font-normal` |
| Button | 14px | 500 | `text-sm font-medium` |

---

## Spacing

Use Tailwind's default spacing scale (4px base):

| Token | Value | Usage |
|-------|-------|-------|
| `1` | 4px | Tight spacing |
| `2` | 8px | Icon gaps, tight padding |
| `3` | 12px | Button padding (vertical) |
| `4` | 16px | Card padding, standard gaps |
| `6` | 24px | Section spacing |
| `8` | 32px | Major section breaks |

---

## Components

### Buttons

```rust
// Primary action
view! {
    <button class="
        bg-accent-600 hover:bg-accent-700 
        text-white font-medium
        px-4 py-2 rounded-lg
        focus:outline-none focus:ring-2 focus:ring-accent-400 focus:ring-offset-2 focus:ring-offset-void-900
        disabled:opacity-50 disabled:cursor-not-allowed
        transition-colors
    ">
        "Unlock Vault"
    </button>
}

// Secondary action
view! {
    <button class="
        bg-void-700 hover:bg-void-600
        text-void-100 font-medium
        px-4 py-2 rounded-lg
        border border-void-600
        focus:outline-none focus:ring-2 focus:ring-accent-400
        transition-colors
    ">
        "Cancel"
    </button>
}

// Danger action
view! {
    <button class="
        bg-transparent hover:bg-danger/10
        text-danger font-medium
        px-4 py-2 rounded-lg
        border border-danger/50
        transition-colors
    ">
        "Delete"
    </button>
}
```

### Input Fields

```rust
view! {
    <div class="space-y-1">
        <label class="block text-sm font-medium text-void-200">
            "Password"
        </label>
        <input
            type="password"
            class="
                w-full px-3 py-2
                bg-void-900 border border-void-600
                text-void-50 placeholder-void-500
                rounded-lg
                focus:outline-none focus:ring-2 focus:ring-accent-500 focus:border-transparent
                transition-colors
            "
            placeholder="Enter your password"
        />
    </div>
}
```

### Cards / Panels

```rust
view! {
    <div class="
        bg-void-900 
        border border-void-700
        rounded-xl
        p-4
        shadow-lg shadow-black/20
    ">
        // Card content
    </div>
}
```

### Status Indicators

```rust
// Locked state
view! {
    <div class="flex items-center gap-2 text-locked">
        <LockIcon class="w-4 h-4" />
        <span class="text-sm">"Vault Locked"</span>
    </div>
}

// Unlocked state
view! {
    <div class="flex items-center gap-2 text-secure">
        <UnlockIcon class="w-4 h-4" />
        <span class="text-sm">"Vault Unlocked"</span>
    </div>
}

// Session warning
view! {
    <div class="flex items-center gap-2 text-warning">
        <ClockIcon class="w-4 h-4" />
        <span class="text-sm">"Session expires in 5 minutes"</span>
    </div>
}
```

### File List Item

```rust
view! {
    <div class="
        flex items-center gap-3
        px-3 py-2
        hover:bg-void-800
        rounded-lg
        cursor-pointer
        transition-colors
    ">
        <FileIcon class="w-5 h-5 text-void-400" />
        <span class="text-void-100 font-mono text-sm truncate flex-1">
            "document.pdf"
        </span>
        <span class="text-void-500 text-xs">
            "2.4 MB"
        </span>
    </div>
}
```

---

## Layout Patterns

### App Shell

```rust
view! {
    <div class="min-h-screen bg-void-950 text-void-100">
        // Header
        <header class="bg-void-900 border-b border-void-700 px-4 py-3">
            <div class="flex items-center justify-between max-w-6xl mx-auto">
                <Logo />
                <SessionStatus />
            </div>
        </header>
        
        // Main content
        <main class="max-w-6xl mx-auto px-4 py-6">
            {children()}
        </main>
    </div>
}
```

### Login Screen

Centered card on dark background:

```rust
view! {
    <div class="min-h-screen bg-void-950 flex items-center justify-center p-4">
        <div class="w-full max-w-md">
            <div class="bg-void-900 border border-void-700 rounded-xl p-6 shadow-xl">
                <h1 class="text-2xl font-semibold text-void-50 text-center mb-6">
                    "Unlock Vault"
                </h1>
                // Form fields...
            </div>
        </div>
    </div>
}
```

---

## Accessibility

- **Focus rings**: Always visible, use accent color
- **Contrast**: All text meets WCAG AA (4.5:1 minimum)
- **Keyboard navigation**: All interactive elements focusable
- **Screen readers**: Use semantic HTML, aria-labels where needed

### Focus Ring Pattern

```css
focus:outline-none focus:ring-2 focus:ring-accent-400 focus:ring-offset-2 focus:ring-offset-void-900
```

---

## Icons

Use a consistent icon library. Recommended options:

- **Heroicons** (MIT, works well with Tailwind)
- **Lucide** (fork of Feather, MIT)

Icon sizing:
- Inline with text: `w-4 h-4`
- Buttons: `w-5 h-5`
- Feature icons: `w-6 h-6`

---

## Animations

Keep animations minimal and purposeful:

```css
/* Standard transition for interactive elements */
transition-colors duration-150

/* Subtle fade for modals */
transition-opacity duration-200
```

**Avoid:**
- Bouncy/playful animations
- Long durations (>300ms)
- Animations that distract from security state

---

## Do's and Don'ts

### Do

- ✅ Use dark backgrounds consistently
- ✅ Make locked/unlocked states immediately obvious
- ✅ Use muted, professional colors
- ✅ Prioritize readability over aesthetics
- ✅ Show clear feedback for all actions

### Don't

- ❌ Use bright/saturated colors for decoration
- ❌ Add animations that feel playful
- ❌ Show sensitive data (paths, keys) in plain text
- ❌ Use inconsistent spacing or colors
- ❌ Hide security status indicators
