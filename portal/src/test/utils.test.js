import { describe, it, expect } from 'vitest'
import { formatTime, formatDuration, toApiDatetime, decodePolyline, modeColor } from '../utils.js'

describe('formatTime', () => {
  it('formats a valid datetime string', () => {
    expect(formatTime('20260409T143000')).toBe('14:30')
  })
  it('returns --:-- for short string', () => {
    expect(formatTime('20260409')).toBe('--:--')
  })
  it('returns --:-- for null', () => {
    expect(formatTime(null)).toBe('--:--')
  })
  it('returns --:-- for undefined', () => {
    expect(formatTime(undefined)).toBe('--:--')
  })
})

describe('formatDuration', () => {
  it('formats minutes only', () => {
    expect(formatDuration(300)).toBe('5 min')
  })
  it('formats hours and minutes', () => {
    expect(formatDuration(3720)).toBe('1h02')
  })
  it('formats zero', () => {
    expect(formatDuration(0)).toBe('0 min')
  })
  it('pads minutes with leading zero', () => {
    expect(formatDuration(3660)).toBe('1h01')
  })
})

describe('toApiDatetime', () => {
  it('converts ISO-like datetime', () => {
    expect(toApiDatetime('2026-04-09T14:30')).toBe('20260409T143000')
  })
  it('handles already clean format', () => {
    expect(toApiDatetime('20260409T1430')).toBe('20260409T143000')
  })
  it('pads short time to 6 digits', () => {
    expect(toApiDatetime('2026-04-09T09:00')).toBe('20260409T090000')
  })
})

describe('decodePolyline', () => {
  it('decodes a known two-point polyline', () => {
    // Two points: decode and check we get 2 coordinates with valid numbers
    const coords = decodePolyline('_p~iF~ps|U_ulLnnqC')
    expect(coords.length).toBe(2)
    expect(typeof coords[0][0]).toBe('number')
    expect(typeof coords[0][1]).toBe('number')
    expect(coords[0][0]).not.toBe(0)
    expect(coords[1][0]).not.toBe(coords[0][0]) // two different points
  })
  it('returns empty array for empty string', () => {
    expect(decodePolyline('')).toEqual([])
  })
})

describe('modeColor', () => {
  it('returns hex color when provided', () => {
    expect(modeColor('metro', 'FFCD00')).toBe('#FFCD00')
  })
  it('returns default for metro without color', () => {
    expect(modeColor('metro', '')).toBe('#4fc3f7')
  })
  it('returns default for rail', () => {
    expect(modeColor('rail', '')).toBe('#e0e0e0')
  })
  it('returns default for unknown mode', () => {
    expect(modeColor('unknown', '')).toBe('#90a4ae')
  })
})
