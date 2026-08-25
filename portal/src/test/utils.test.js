import { describe, it, expect } from 'vitest'
import { formatTime, formatDuration, secondsBetween, parseApiDateTime, toApiDatetime, decodePolyline, modeColor,
  disruptionInstantToApi, disruptionInstantToInput, disruptionStartDefault } from '../utils.js'

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

describe('parseApiDateTime', () => {
  it('parses a valid datetime to UTC epoch ms', () => {
    expect(parseApiDateTime('20260530T180300')).toBe(Date.UTC(2026, 4, 30, 18, 3, 0))
  })
  it('returns null for short or null input', () => {
    expect(parseApiDateTime('20260530')).toBeNull()
    expect(parseApiDateTime(null)).toBeNull()
  })
})

describe('secondsBetween', () => {
  it('computes a positive platform wait', () => {
    // arrival 18:03:00 → next departure 18:07:49 = 289 s
    expect(secondsBetween('20260530T180300', '20260530T180749')).toBe(289)
  })
  it('returns 0 for contiguous sections', () => {
    expect(secondsBetween('20260530T180300', '20260530T180300')).toBe(0)
  })
  it('returns null when either timestamp is invalid', () => {
    expect(secondsBetween('bad', '20260530T180300')).toBeNull()
    expect(secondsBetween('20260530T180300', null)).toBeNull()
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

describe('disruptionInstantToApi', () => {
  it('appends seconds to a datetime-local value', () => {
    expect(disruptionInstantToApi('2026-09-01T22:00')).toBe('2026-09-01T22:00:00')
  })

  it('leaves a value that already carries seconds alone', () => {
    expect(disruptionInstantToApi('2026-09-01T22:00:30')).toBe('2026-09-01T22:00:30')
  })

  it('maps an empty value to null, which the API reads as "no end date"', () => {
    expect(disruptionInstantToApi('')).toBeNull()
    expect(disruptionInstantToApi(null)).toBeNull()
    expect(disruptionInstantToApi(undefined)).toBeNull()
  })
})

describe('disruptionInstantToInput', () => {
  it('drops the seconds an input cannot show', () => {
    expect(disruptionInstantToInput('2026-09-01T22:00:00')).toBe('2026-09-01T22:00')
  })

  it('maps an absent value to an empty field', () => {
    expect(disruptionInstantToInput(null)).toBe('')
    expect(disruptionInstantToInput('')).toBe('')
  })

  it('round-trips with disruptionInstantToApi', () => {
    const api = '2026-09-01T22:00:00'
    expect(disruptionInstantToApi(disruptionInstantToInput(api))).toBe(api)
  })
})

describe('disruptionStartDefault', () => {
  it('formats an instant from its local components', () => {
    // Constructed and read back through local getters, so the expectation
    // holds in any timezone.
    expect(disruptionStartDefault(new Date(2026, 8, 1, 22, 5))).toBe('2026-09-01T22:05')
  })

  it('pads month, day, hour and minute', () => {
    expect(disruptionStartDefault(new Date(2026, 0, 2, 3, 4))).toBe('2026-01-02T03:04')
  })

  it('keeps a just-past-midnight start on its own local day', () => {
    // The trap toISOString() would fall into: east of Greenwich it would
    // report the previous day, making the disruption look already over.
    expect(disruptionStartDefault(new Date(2026, 0, 1, 0, 30))).toBe('2026-01-01T00:30')
  })

  it('produces a value disruptionInstantToApi accepts', () => {
    const value = disruptionStartDefault(new Date(2026, 8, 1, 22, 5))
    expect(disruptionInstantToApi(value)).toBe('2026-09-01T22:05:00')
  })
})
