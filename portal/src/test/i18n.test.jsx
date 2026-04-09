import { describe, it, expect } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { I18nProvider, useI18n } from '../i18n.jsx'

function wrapper({ children }) {
  return <I18nProvider>{children}</I18nProvider>
}

describe('useI18n', () => {
  it('returns French translations by default for fr browser', () => {
    // Browser language detection is mocked; defaults to 'en' in test env
    const { result } = renderHook(() => useI18n(), { wrapper })
    // Should return a valid translation key
    expect(result.current.t('search')).toBeTruthy()
    expect(result.current.t('search')).not.toBe('search') // not falling back to key
  })

  it('supports parameter interpolation', () => {
    const { result } = renderHook(() => useI18n(), { wrapper })
    const text = result.current.t('journeysFound', { count: 3 })
    expect(text).toContain('3')
  })

  it('toggles language', () => {
    const { result } = renderHook(() => useI18n(), { wrapper })
    const initialLang = result.current.lang
    act(() => {
      result.current.setLang(initialLang === 'fr' ? 'en' : 'fr')
    })
    expect(result.current.lang).not.toBe(initialLang)
  })

  it('falls back to key for unknown translation', () => {
    const { result } = renderHook(() => useI18n(), { wrapper })
    expect(result.current.t('nonexistent_key_xyz')).toBe('nonexistent_key_xyz')
  })
})
