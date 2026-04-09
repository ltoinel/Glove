// Pure utility functions extracted for testability.

export function formatTime(dt) {
  if (!dt || dt.length < 15) return '--:--'
  return dt.slice(9, 11) + ':' + dt.slice(11, 13)
}

export function formatDuration(seconds) {
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  if (h > 0) return `${h}h${m.toString().padStart(2, '0')}`
  return `${m} min`
}

export function toApiDatetime(val) {
  const clean = val.replace(/[-:]/g, '')
  const tIdx = clean.indexOf('T')
  if (tIdx === 8) {
    const time = clean.slice(9).padEnd(6, '0')
    return clean.slice(0, 9) + time
  }
  return clean.slice(0, 8) + 'T' + clean.slice(8).padEnd(6, '0')
}

export function decodePolyline(encoded, precision = 6) {
  const factor = Math.pow(10, precision)
  const result = []
  let index = 0, lat = 0, lng = 0
  while (index < encoded.length) {
    let shift = 0, byte, val = 0
    do { byte = encoded.charCodeAt(index++) - 63; val |= (byte & 0x1f) << shift; shift += 5 } while (byte >= 0x20)
    lat += (val & 1) ? ~(val >> 1) : (val >> 1)
    shift = 0; val = 0
    do { byte = encoded.charCodeAt(index++) - 63; val |= (byte & 0x1f) << shift; shift += 5 } while (byte >= 0x20)
    lng += (val & 1) ? ~(val >> 1) : (val >> 1)
    result.push([lat / factor, lng / factor])
  }
  return result
}

export function modeColor(mode, color) {
  if (color) return `#${color}`
  switch (mode) {
    case 'metro': return '#4fc3f7'
    case 'rail': return '#e0e0e0'
    case 'tramway': return '#66bb6a'
    case 'bus': return '#aed581'
    default: return '#90a4ae'
  }
}
