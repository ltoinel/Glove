import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { ThemeProvider, createTheme, CssBaseline } from '@mui/material'
import { I18nProvider } from './i18n.jsx'
import App from './App.jsx'

const theme = createTheme({
  palette: {
    mode: 'dark',
    primary: { main: '#00e5ff' },
    secondary: { main: '#ffb800' },
    background: { default: '#0a0a12', paper: '#12121e' },
    text: { primary: '#e8e6f0', secondary: '#8b89a0' },
    divider: 'rgba(255, 255, 255, 0.06)',
    success: { main: '#00e676' },
    error: { main: '#ff5252' },
    warning: { main: '#ffb800' },
  },
  typography: {
    fontFamily: '"Figtree", "Segoe UI", system-ui, sans-serif',
    h6: { fontFamily: '"Syne", sans-serif', fontWeight: 700, letterSpacing: '-0.02em' },
    subtitle1: { fontFamily: '"Syne", sans-serif', fontWeight: 600 },
    subtitle2: { fontFamily: '"Syne", sans-serif', fontWeight: 600 },
    overline: { fontFamily: '"Syne", sans-serif', fontWeight: 700, letterSpacing: '0.12em' },
    body2: { fontSize: '0.875rem' },
    caption: { fontSize: '0.75rem' },
  },
  shape: { borderRadius: 12 },
  components: {
    MuiCssBaseline: {
      styleOverrides: {
        body: { backgroundColor: '#0a0a12' },
      },
    },
    MuiButton: {
      styleOverrides: {
        root: {
          textTransform: 'none',
          fontFamily: '"Syne", sans-serif',
          fontWeight: 600,
          borderRadius: 10,
          padding: '10px 20px',
          letterSpacing: '0.01em',
        },
      },
    },
    MuiPaper: {
      styleOverrides: {
        root: {
          backgroundImage: 'none',
          backgroundColor: 'rgba(20, 20, 35, 0.75)',
          backdropFilter: 'blur(16px)',
          border: '1px solid rgba(255, 255, 255, 0.06)',
        },
      },
    },
    MuiCard: {
      styleOverrides: {
        root: {
          borderRadius: 14,
          transition: 'all 0.25s cubic-bezier(0.4, 0, 0.2, 1)',
          backgroundImage: 'none',
          backgroundColor: 'rgba(20, 20, 35, 0.6)',
          backdropFilter: 'blur(12px)',
          border: '1px solid rgba(255, 255, 255, 0.06)',
        },
      },
    },
    MuiOutlinedInput: {
      styleOverrides: {
        root: {
          borderRadius: 10,
          '& .MuiOutlinedInput-notchedOutline': {
            borderColor: 'rgba(255, 255, 255, 0.08)',
          },
          '&:hover .MuiOutlinedInput-notchedOutline': {
            borderColor: 'rgba(0, 229, 255, 0.3)',
          },
          '&.Mui-focused .MuiOutlinedInput-notchedOutline': {
            borderColor: '#00e5ff',
            boxShadow: '0 0 0 3px rgba(0, 229, 255, 0.1)',
          },
        },
      },
    },
    MuiChip: {
      styleOverrides: {
        root: { borderRadius: 8 },
      },
    },
    MuiAutocomplete: {
      styleOverrides: {
        paper: {
          backgroundColor: 'rgba(18, 18, 32, 0.95)',
          backdropFilter: 'blur(20px)',
          border: '1px solid rgba(255, 255, 255, 0.08)',
          boxShadow: '0 12px 40px rgba(0, 0, 0, 0.5)',
        },
        option: {
          '&:hover': {
            backgroundColor: 'rgba(0, 229, 255, 0.08) !important',
          },
          '&[aria-selected="true"]': {
            backgroundColor: 'rgba(0, 229, 255, 0.12) !important',
          },
        },
      },
    },
    MuiAlert: {
      styleOverrides: {
        root: {
          backdropFilter: 'blur(12px)',
          border: '1px solid rgba(255, 255, 255, 0.06)',
        },
      },
    },
    MuiTooltip: {
      styleOverrides: {
        tooltip: {
          backgroundColor: 'rgba(12, 12, 20, 0.92)',
          backdropFilter: 'blur(8px)',
          border: '1px solid rgba(255, 255, 255, 0.08)',
          fontFamily: '"Figtree", sans-serif',
          fontSize: '0.75rem',
        },
      },
    },
  },
})

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <I18nProvider>
      <ThemeProvider theme={theme}>
        <CssBaseline />
        <App />
      </ThemeProvider>
    </I18nProvider>
  </StrictMode>,
)
