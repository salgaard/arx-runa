/** @type {import('tailwindcss').Config} */
module.exports = {
  content: {
    files: ["*.html", "./src/**/*.rs"],
  },
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // Arx Runa primary palette - dark blue-gray
        'void': {
          50: '#f0f4f8',
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
        // Accent - muted teal (trust/security connotation)
        'accent': {
          400: '#4fd1c5',
          500: '#38b2ac',
          600: '#319795',
          700: '#2c7a7b',
        },
        // Status colors - used sparingly
        'secure': '#10b981', // Green - unlocked, success
        'locked': '#6b7280', // Gray - locked state
        'warning': '#f59e0b', // Amber - session timeout, caution
        'danger': '#ef4444', // Red - errors only
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'sans-serif'],
        mono: ['JetBrains Mono', 'Consolas', 'Monaco', 'monospace'],
      },
      boxShadow: {
        'glow': '0 0 20px rgba(56, 178, 172, 0.15)',
      },
    },
  },
  plugins: [],
}
