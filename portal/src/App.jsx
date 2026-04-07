import { useState, useEffect, useRef, useCallback } from 'react'
import {
  Typography, Paper, TextField, Button, Slider, Checkbox, FormControlLabel,
  Box, Card, CardContent, CardActionArea, Chip, Collapse, Alert,
  CircularProgress, Divider, Stack, IconButton, Tooltip, Autocomplete, alpha,
} from '@mui/material'
import {
  Search, SwapVert, DirectionsBus, Train, Tram, Subway,
  ExpandMore, ExpandLess, TransferWithinAStation, DirectionsWalk,
  NearMe, ArrowRightAlt, Place, AccessTime, Settings,
  Route, Timer, MultipleStop, Storage, CalendarMonth, Close, Language, Api,
  DirectionsBike, DirectionsCar, MonitorHeart, Memory, Speed, Dns, Http,
  Stairs, Elevator, MeetingRoom, TurnLeft, TurnRight, Straight, UTurnLeft,
  RoundaboutLeft, Flag, MyLocation, ForkLeft, ForkRight, MergeType,
} from '@mui/icons-material'
import SwaggerUI from 'swagger-ui-react'
import 'swagger-ui-react/swagger-ui.css'
import { MapContainer, TileLayer, Polyline, CircleMarker, Marker, Tooltip as LTooltip, useMap } from 'react-leaflet'
import L from 'leaflet'
import 'leaflet/dist/leaflet.css'
import { LocalizationProvider } from '@mui/x-date-pickers/LocalizationProvider'
import { AdapterDayjs } from '@mui/x-date-pickers/AdapterDayjs'
import { DatePicker } from '@mui/x-date-pickers/DatePicker'
import { TimePicker } from '@mui/x-date-pickers/TimePicker'
import dayjs from 'dayjs'
import 'dayjs/locale/fr'
import { useI18n } from './i18n.jsx'

// Origin marker — pulsing radar dot
const originIcon = L.divIcon({
  html: `<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 40 40">
    <defs>
      <radialGradient id="og" cx="50%" cy="50%" r="50%">
        <stop offset="0%" stop-color="#00e676" stop-opacity="0.35"/>
        <stop offset="100%" stop-color="#00e676" stop-opacity="0"/>
      </radialGradient>
    </defs>
    <circle cx="20" cy="20" r="18" fill="url(#og)">
      <animate attributeName="r" values="10;18;10" dur="2.5s" repeatCount="indefinite"/>
      <animate attributeName="opacity" values="1;0.4;1" dur="2.5s" repeatCount="indefinite"/>
    </circle>
    <circle cx="20" cy="20" r="8" fill="#0a0a12" stroke="#00e676" stroke-width="2.5"/>
    <circle cx="20" cy="20" r="4" fill="#00e676"/>
  </svg>`,
  iconSize: [40, 40], iconAnchor: [20, 20], className: '',
})

// Destination marker — modern teardrop pin
const destinationIcon = L.divIcon({
  html: `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="44" viewBox="0 0 32 44">
    <defs>
      <linearGradient id="dg" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stop-color="#ff5252"/>
        <stop offset="100%" stop-color="#d32f2f"/>
      </linearGradient>
      <filter id="ds" x="-20%" y="-10%" width="140%" height="130%">
        <feDropShadow dx="0" dy="2" stdDeviation="2.5" flood-color="#000" flood-opacity="0.4"/>
      </filter>
    </defs>
    <g filter="url(#ds)">
      <path d="M16 2C8.82 2 3 7.82 3 15c0 9.75 13 25 13 25s13-15.25 13-25C29 7.82 23.18 2 16 2z" fill="url(#dg)"/>
      <circle cx="16" cy="15" r="5.5" fill="#0a0a12"/>
      <circle cx="16" cy="15" r="2.5" fill="#fff"/>
    </g>
  </svg>`,
  iconSize: [32, 44], iconAnchor: [16, 42], className: '',
})
const DEFAULT_CENTER = [48.8566, 2.3522]
const DEFAULT_ZOOM = 11

// --- Polyline smoothing (Catmull-Rom spline) ---

function smoothLine(coords, numPoints = 6) {
  if (coords.length < 3) return coords
  const result = []
  const pts = [coords[0], ...coords, coords[coords.length - 1]]
  for (let i = 1; i < pts.length - 2; i++) {
    const p0 = pts[i - 1], p1 = pts[i], p2 = pts[i + 1], p3 = pts[i + 2]
    for (let t = 0; t < numPoints; t++) {
      const f = t / numPoints
      const f2 = f * f, f3 = f2 * f
      const lat = 0.5 * ((2 * p1[0]) + (-p0[0] + p2[0]) * f + (2 * p0[0] - 5 * p1[0] + 4 * p2[0] - p3[0]) * f2 + (-p0[0] + 3 * p1[0] - 3 * p2[0] + p3[0]) * f3)
      const lon = 0.5 * ((2 * p1[1]) + (-p0[1] + p2[1]) * f + (2 * p0[1] - 5 * p1[1] + 4 * p2[1] - p3[1]) * f2 + (-p0[1] + 3 * p1[1] - 3 * p2[1] + p3[1]) * f3)
      result.push([lat, lon])
    }
  }
  result.push(coords[coords.length - 1])
  return result
}

// --- Decode Valhalla encoded polyline (precision 6) ---

function decodePolyline(encoded, precision = 6) {
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

// --- Utilities ---

function formatTime(dt) {
  if (!dt || dt.length < 15) return '--:--'
  return dt.slice(9, 11) + ':' + dt.slice(11, 13)
}

function formatDuration(seconds) {
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  if (h > 0) return `${h}h${m.toString().padStart(2, '0')}`
  return `${m} min`
}

function defaultDatetime() {
  const now = new Date()
  const pad = (n) => n.toString().padStart(2, '0')
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`
}

function toApiDatetime(val) {
  const clean = val.replace(/[-:]/g, '')
  const tIdx = clean.indexOf('T')
  if (tIdx === 8) {
    const time = clean.slice(9).padEnd(6, '0')
    return clean.slice(0, 9) + time
  }
  return clean.slice(0, 8) + 'T' + clean.slice(8).padEnd(6, '0')
}

// --- Elevation-colored segments for bike routes ---

function elevationSegments(coords, heights) {
  if (!heights || heights.length < 2 || coords.length < 2) return []
  // Map heights to coords (heights are sampled, so interpolate indices)
  const step = (coords.length - 1) / (heights.length - 1)
  const segments = []
  for (let i = 0; i < coords.length - 1; i++) {
    const hIdx = i / step
    const lo = Math.floor(hIdx)
    const hi = Math.min(lo + 1, heights.length - 1)
    const frac = hIdx - lo
    const h0 = heights[lo] + frac * (heights[hi] - heights[lo])
    const nextHIdx = (i + 1) / step
    const nlo = Math.floor(nextHIdx)
    const nhi = Math.min(nlo + 1, heights.length - 1)
    const nfrac = nextHIdx - nlo
    const h1 = heights[nlo] + nfrac * (heights[nhi] - heights[nlo])
    const diff = h1 - h0
    // Color by slope: green=descent, yellow=flat, orange/red=climb
    let color
    if (diff <= -2) color = '#4caf50'       // descent
    else if (diff <= -0.5) color = '#66bb6a' // slight descent
    else if (diff < 0.5) color = '#ffeb3b'   // flat
    else if (diff < 2) color = '#ff9800'     // slight climb
    else color = '#f44336'                   // steep climb
    segments.push({ positions: [coords[i], coords[i + 1]], color })
  }
  // Merge consecutive segments with same color
  const merged = [segments[0]]
  for (let i = 1; i < segments.length; i++) {
    const prev = merged[merged.length - 1]
    if (segments[i].color === prev.color) {
      prev.positions.push(segments[i].positions[1])
    } else {
      merged.push({ positions: [segments[i].positions[0], segments[i].positions[1]], color: segments[i].color })
    }
  }
  return merged
}

function modeColor(mode, color) {
  if (color) return `#${color}`
  switch (mode) {
    case 'metro': return '#4fc3f7'
    case 'rail': return '#e0e0e0'
    case 'tramway': return '#66bb6a'
    case 'bus': return '#aed581'
    default: return '#90a4ae'
  }
}

// --- Recent places history ---

const RECENT_PLACES_KEY = 'glove_recent_places'
const MAX_RECENT_PLACES = 50

function getRecentPlaces() {
  try {
    return JSON.parse(localStorage.getItem(RECENT_PLACES_KEY)) || []
  } catch { return [] }
}

function saveRecentPlace(place) {
  if (!place?.id || !place?.name) return
  const recent = getRecentPlaces().filter(p => p.id !== place.id)
  recent.unshift(place)
  if (recent.length > MAX_RECENT_PLACES) recent.length = MAX_RECENT_PLACES
  localStorage.setItem(RECENT_PLACES_KEY, JSON.stringify(recent))
}

// --- Autocomplete ---

function useDebouncedFetch(delay = 250) {
  const timerRef = useRef(null)
  return useCallback((query, callback) => {
    clearTimeout(timerRef.current)
    if (!query || query.length < 2) { callback([]); return }
    timerRef.current = setTimeout(async () => {
      try {
        const res = await fetch(`/api/places?q=${encodeURIComponent(query)}&limit=10`)
        const data = await res.json()
        callback(data.places || [])
      } catch { callback([]) }
    }, delay)
  }, [delay])
}

function PlaceAutocomplete({ label, value, onChange, icon, placeholder }) {
  const { t } = useI18n()
  const [inputValue, setInputValue] = useState('')
  const [options, setOptions] = useState([])
  const [loading, setLoading] = useState(false)
  const fetchPlaces = useDebouncedFetch()

  const handleInputChange = useCallback((_, newInput) => {
    setInputValue(newInput)
    if (!newInput || newInput.length < 2) {
      const recent = getRecentPlaces()
      setOptions(value ? [value, ...recent.filter(p => p.id !== value.id)] : recent)
      setLoading(false)
      return
    }
    setLoading(true)
    fetchPlaces(newInput, (results) => { setOptions(results); setLoading(false) })
  }, [fetchPlaces, value])

  const displayOptions = (!inputValue || inputValue.length < 2)
    ? (() => { const recent = getRecentPlaces(); return value ? [value, ...recent.filter(p => p.id !== value.id)] : recent })()
    : options

  return (
    <Autocomplete
      fullWidth size="small" options={displayOptions} openOnFocus
      getOptionLabel={(opt) => typeof opt === 'string' ? opt : opt.name || ''}
      isOptionEqualToValue={(opt, val) => opt.id === val.id}
      filterOptions={(x) => x}
      value={value}
      onChange={(_, newVal) => onChange(newVal)}
      onInputChange={handleInputChange}
      loading={loading}
      noOptionsText={t('typeToSearch')}
      loadingText={t('loadingSearch')}
      renderInput={(params) => (
        <TextField {...params} label={label} placeholder={placeholder || t('searchPlaceholder')}
          slotProps={{
            input: {
              ...params.InputProps,
              startAdornment: (
                <>{icon}<Box sx={{ mr: 0.5 }} />{params.InputProps.startAdornment}</>
              ),
            },
          }}
        />
      )}
      groupBy={(option) => {
        if (!inputValue || inputValue.length < 2) return 'recent'
        return option.type === 'stop' ? 'stops' : 'addresses'
      }}
      renderGroup={(params) => (
        <li key={params.key}>
          <Box sx={{
            px: 1.5, py: 0.5,
            bgcolor: 'rgba(255,255,255,0.03)',
            borderBottom: '1px solid rgba(255,255,255,0.04)',
          }}>
            <Typography variant="caption" sx={{
              fontFamily: '"Syne", sans-serif', fontWeight: 700, fontSize: 10,
              letterSpacing: 1.5, textTransform: 'uppercase',
              color: params.group === 'recent' ? '#b388ff' : params.group === 'stops' ? '#00e5ff' : '#ffb800',
            }}>
              {params.group === 'recent' ? t('recentPlaces') : params.group === 'stops' ? t('stopsLabel') : t('addresses')}
            </Typography>
          </Box>
          {params.children}
        </li>
      )}
      renderOption={(props, option) => {
        const { key, ...rest } = props
        const isStop = option.type === 'stop'
        return (
          <Box component="li" key={key} {...rest}
            sx={{ display: 'flex', alignItems: 'center', gap: 1.5, py: 0.8, px: 1.5 }}>
            <Box sx={{
              width: 28, height: 28, borderRadius: isStop ? '8px' : '50%',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              bgcolor: isStop ? 'rgba(0, 229, 255, 0.1)' : 'rgba(255, 184, 0, 0.1)',
              border: '1px solid',
              borderColor: isStop ? 'rgba(0, 229, 255, 0.25)' : 'rgba(255, 184, 0, 0.25)',
              flexShrink: 0,
            }}>
              {isStop
                ? <DirectionsBus sx={{ fontSize: 15, color: '#00e5ff' }} />
                : <Place sx={{ fontSize: 15, color: '#ffb800' }} />
              }
            </Box>
            <Typography variant="body2" fontWeight={500} noWrap>{option.name}</Typography>
          </Box>
        )
      }}
    />
  )
}

// --- Map helpers ---

function FitBounds({ bounds }) {
  const map = useMap()
  useEffect(() => {
    if (bounds && bounds.length >= 2) map.fitBounds(bounds, { padding: [60, 60], maxZoom: 15 })
  }, [map, bounds])
  return null
}

function FlyToPoint({ point }) {
  const map = useMap()
  useEffect(() => {
    if (point) map.flyTo(point, 14, { duration: 0.8 })
  }, [map, point])
  return null
}

function extractMapData(journey) {
  const lines = []
  const stopPoints = []
  const labeledStops = []
  const sections = journey.sections
  const ptSections = sections.filter(s => s.type === 'public_transport')

  for (const section of sections) {
    // Walking legs (first/last mile via Valhalla)
    if (section.type === 'street_network' && section.shape) {
      const coords = decodePolyline(section.shape)
      if (coords.length >= 2) lines.push({ coords, color: '#90a4ae', dashed: true })
      continue
    }
    // Transfer walking legs (straight line between stops)
    if (section.type === 'transfer') {
      const fromCoord = section.from?.stop_point?.coord
      const toCoord = section.to?.stop_point?.coord
      if (fromCoord && toCoord) {
        lines.push({
          coords: [[fromCoord.lat, fromCoord.lon], [toCoord.lat, toCoord.lon]],
          color: '#90a4ae', dashed: true,
        })
      }
      continue
    }
    if (section.type !== 'public_transport' || !section.stop_date_times) continue
    const di = section.display_informations
    const color = modeColor(di?.commercial_mode, di?.color)
    const coords = section.stop_date_times
      .filter(sdt => sdt.stop_point?.coord)
      .map(sdt => [sdt.stop_point.coord.lat, sdt.stop_point.coord.lon])
    if (coords.length >= 2) lines.push({ coords, color })
    for (const sdt of section.stop_date_times) {
      const sp = sdt.stop_point
      if (!sp?.coord) continue
      stopPoints.push({ pos: [sp.coord.lat, sp.coord.lon], name: sp.name, color })
    }
  }

  if (ptSections.length > 0) {
    const firstPt = ptSections[0]
    if (firstPt.from?.stop_point?.coord) {
      const sp = firstPt.from.stop_point
      labeledStops.push({ pos: [sp.coord.lat, sp.coord.lon], name: sp.name, type: 'origin' })
    }
    const lastPt = ptSections[ptSections.length - 1]
    if (lastPt.to?.stop_point?.coord) {
      const sp = lastPt.to.stop_point
      labeledStops.push({ pos: [sp.coord.lat, sp.coord.lon], name: sp.name, type: 'destination' })
    }
  }

  for (const section of sections) {
    if (section.type !== 'transfer') continue
    const idx = sections.indexOf(section)
    const before = sections[idx - 1]
    const after = sections[idx + 1]
    if (before?.type === 'public_transport' && after?.type === 'public_transport') {
      const sp = section.from?.stop_point
      if (sp?.coord) labeledStops.push({ pos: [sp.coord.lat, sp.coord.lon], name: sp.name, type: 'transfer' })
    }
  }

  return { lines, stopPoints, labeledStops }
}

// --- Journey card ---

const TAG_COLORS = {
  fastest: '#00e5ff',
  least_transfers: '#ffb800',
  least_walking: '#b388ff',
}

function JourneyCard({ journey, selected, onSelect, animDelay }) {
  const { t } = useI18n()
  const [open, setOpen] = useState(false)

  return (
    <Card
      sx={{
        mb: 1.5,
        border: '1px solid',
        borderColor: selected ? 'rgba(0, 229, 255, 0.4)' : 'rgba(255, 255, 255, 0.04)',
        bgcolor: selected ? 'rgba(0, 229, 255, 0.06)' : 'rgba(20, 20, 35, 0.5)',
        boxShadow: selected ? '0 0 24px rgba(0, 229, 255, 0.1), inset 0 1px 0 rgba(255,255,255,0.05)' : 'inset 0 1px 0 rgba(255,255,255,0.03)',
        '&:hover': {
          borderColor: selected ? 'rgba(0, 229, 255, 0.5)' : 'rgba(255, 255, 255, 0.1)',
          bgcolor: selected ? 'rgba(0, 229, 255, 0.08)' : 'rgba(20, 20, 35, 0.7)',
          transform: 'translateY(-1px)',
        },
        animation: 'cardSlideIn 0.4s cubic-bezier(0.16, 1, 0.3, 1) both',
        animationDelay: `${animDelay}ms`,
      }}
      elevation={0}
    >
      <CardActionArea onClick={() => { setOpen(!open); onSelect() }}>
        <CardContent sx={{ py: 1.5, px: 2.5 }}>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.5 }}>
            <Box sx={{ flex: 1, minWidth: 0 }}>
              <Typography variant="body2" fontWeight={600} sx={{ fontFamily: '"Syne", sans-serif' }}>
                {formatTime(journey.departure_date_time)}
                <ArrowRightAlt sx={{ verticalAlign: 'middle', mx: 0.5, opacity: 0.3 }} fontSize="small" />
                {formatTime(journey.arrival_date_time)}
              </Typography>
              <Stack direction="row" spacing={0.5} sx={{ mt: 0.5 }} flexWrap="wrap" useFlexGap alignItems="center">
                {journey.sections.filter(s => s.type === 'public_transport' || s.type === 'street_network' || s.type === 'transfer').map((s, i) => {
                  const chevron = i > 0 ? <Typography variant="caption" sx={{ color: 'text.disabled', mx: 0.2, fontSize: 10 }}>›</Typography> : null
                  if (s.type === 'public_transport' && s.display_informations) {
                    const di = s.display_informations
                    const bg = modeColor(di.commercial_mode, di.color)
                    const fg = di.text_color ? `#${di.text_color}` : '#fff'
                    return (
                      <Box key={i} sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.3 }}>
                        {chevron}
                        <Chip label={di.label || di.commercial_mode}
                          size="small"
                          sx={{
                            bgcolor: bg, color: fg, fontWeight: 700, fontSize: 11, height: 22,
                            boxShadow: `0 0 8px ${alpha(bg, 0.3)}`,
                            '& .MuiChip-label': { px: 0.8 },
                          }} />
                      </Box>
                    )
                  }
                  if ((s.type === 'street_network' || s.type === 'transfer') && s.duration > 0) {
                    const isTransfer = s.type === 'transfer'
                    const mins = Math.floor(s.duration / 60)
                    return (
                      <Box key={i} sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.3 }}>
                        {chevron}
                        {isTransfer
                          ? <TransferWithinAStation sx={{ fontSize: 14, color: '#ffb800' }} />
                          : <DirectionsWalk sx={{ fontSize: 14, color: 'text.disabled' }} />}
                        <Typography variant="caption" sx={{ fontSize: 10, color: isTransfer ? '#ffb800' : 'text.disabled', fontWeight: 600 }}>
                          {mins}'
                        </Typography>
                      </Box>
                    )
                  }
                  return null
                })}
              </Stack>
              {journey.tags?.length > 0 && (
                <Stack direction="row" spacing={0.5} sx={{ mt: 0.5 }} flexWrap="wrap" useFlexGap>
                  {journey.tags.map((tag) => {
                    const color = TAG_COLORS[tag]
                    const label = t(tag)
                    if (!color) return null
                    return (
                      <Chip key={tag} label={label} size="small" variant="outlined"
                        sx={{ height: 20, fontSize: 10, fontWeight: 600, color, borderColor: alpha(color, 0.4),
                          bgcolor: alpha(color, 0.06) }} />
                    )
                  })}
                </Stack>
              )}
            </Box>

            <Box sx={{ textAlign: 'right', flexShrink: 0, pl: 1 }}>
              <Typography variant="body2" fontWeight={800} lineHeight={1.2} color="text.primary"
                sx={{ fontFamily: '"Syne", sans-serif', fontSize: 14 }}>
                {formatDuration(journey.duration)}
              </Typography>
              {journey.nb_transfers > 0 && (
                <Typography variant="caption" sx={{ color: '#ffb800' }} fontWeight={600}>
                  {journey.nb_transfers} {t('transfers')}
                </Typography>
              )}
            </Box>
            {open ? <ExpandLess fontSize="small" sx={{ color: 'text.secondary' }} />
              : <ExpandMore fontSize="small" sx={{ color: 'text.secondary' }} />}
          </Box>
        </CardContent>
      </CardActionArea>

      <Collapse in={open}>
        <Divider sx={{ borderColor: 'rgba(255,255,255,0.04)' }} />
        <Box sx={{ px: 2.5, py: 1.5 }}>
          {journey.sections.map((s, i) => {
            const isPt = s.type === 'public_transport'
            const di = s.display_informations
            const lineColor = isPt ? modeColor(di?.commercial_mode, di?.color) : 'rgba(255,255,255,0.08)'
            return (
              <Box key={i} sx={{
                display: 'flex', gap: 1.5, py: 1,
                borderBottom: i < journey.sections.length - 1 ? '1px solid' : 'none',
                borderColor: 'rgba(255,255,255,0.04)',
              }}>
                <Typography variant="caption" fontWeight={600}
                  sx={{ width: 40, flexShrink: 0, pt: 0.2, color: 'primary.main', fontFamily: '"Syne", sans-serif' }}>
                  {formatTime(s.departure_date_time)}
                </Typography>
                <Box sx={{
                  width: 3, borderRadius: 2, bgcolor: lineColor, flexShrink: 0,
                  boxShadow: isPt ? `0 0 6px ${alpha(lineColor, 0.4)}` : 'none',
                }} />
                <Box sx={{ flex: 1, minWidth: 0 }}>
                  <Typography variant="caption" fontWeight={600} color="text.primary">
                    {isPt ? <>{t(`mode_${di?.commercial_mode}`) || di?.commercial_mode} <strong>{di?.label}</strong></> : s.type === 'street_network' ? (
                      <Box component="span" sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}>
                        <DirectionsWalk sx={{ fontSize: 14, color: '#90a4ae' }} /> {t('walkToStation')}
                      </Box>
                    ) : (
                      <Box component="span" sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}>
                        <TransferWithinAStation sx={{ fontSize: 14, color: '#ffb800' }} /> {t('transfer')}
                      </Box>
                    )}
                  </Typography>
                  <Typography variant="caption" color="text.secondary" display="block" noWrap>
                    {s.from.name} → {s.to.name}
                  </Typography>
                  {isPt && di?.direction && (
                    <Typography variant="caption" color="text.disabled" display="block" fontStyle="italic" noWrap
                      sx={{ opacity: 0.6 }}>
                      {t('direction')} {di.direction}
                    </Typography>
                  )}
                  {s.type === 'street_network' && s.maneuvers && (
                    <ManeuverList maneuvers={s.maneuvers} color="#90a4ae" />
                  )}
                </Box>
                <Typography variant="caption" color="text.secondary" sx={{ flexShrink: 0, pt: 0.2, opacity: 0.7 }}>
                  {formatDuration(s.duration)}
                </Typography>
              </Box>
            )
          })}
        </Box>
      </Collapse>
    </Card>
  )
}

// Valhalla maneuver type → icon mapping
// See https://valhalla.github.io/valhalla/api/turn-by-turn/api-reference/
function maneuverIcon(type) {
  const sx = { fontSize: 16, color: 'text.secondary' }
  switch (type) {
    case 0: return <MyLocation sx={sx} />          // kNone / depart
    case 1: return <Straight sx={sx} />             // kStart
    case 2: return <Straight sx={sx} />             // kStartRight
    case 3: return <Straight sx={sx} />             // kStartLeft
    case 4: return <Flag sx={sx} />                 // kDestination
    case 5: return <Flag sx={sx} />                 // kDestinationRight
    case 6: return <Flag sx={sx} />                 // kDestinationLeft
    case 7: return <Straight sx={sx} />             // kBecomes
    case 8: return <Straight sx={sx} />             // kContinue
    case 9: return <TurnRight sx={{ ...sx, transform: 'rotate(-45deg)' }} /> // kSlightRight
    case 10: return <TurnRight sx={sx} />           // kRight
    case 11: return <TurnRight sx={{ ...sx, transform: 'rotate(45deg)' }} /> // kSharpRight
    case 12: return <UTurnLeft sx={{ ...sx, transform: 'scaleX(-1)' }} />   // kUturnRight
    case 13: return <UTurnLeft sx={sx} />           // kUturnLeft
    case 14: return <TurnLeft sx={{ ...sx, transform: 'rotate(45deg)' }} /> // kSharpLeft
    case 15: return <TurnLeft sx={sx} />            // kLeft
    case 16: return <TurnLeft sx={{ ...sx, transform: 'rotate(-45deg)' }} /> // kSlightLeft
    case 17: return <Straight sx={sx} />            // kRampStraight
    case 18: return <TurnRight sx={sx} />           // kRampRight
    case 19: return <TurnLeft sx={sx} />            // kRampLeft
    case 20: return <Straight sx={sx} />            // kExitRight
    case 21: return <Straight sx={sx} />            // kExitLeft
    case 22: return <Straight sx={sx} />            // kStayStraight
    case 23: return <ForkRight sx={sx} />           // kStayRight
    case 24: return <ForkLeft sx={sx} />            // kStayLeft
    case 25: return <MergeType sx={sx} />           // kMerge
    case 26: return <RoundaboutLeft sx={sx} />      // kRoundaboutEnter
    case 27: return <RoundaboutLeft sx={sx} />      // kRoundaboutExit
    case 28: return <DirectionsBus sx={sx} />       // kFerryEnter
    case 29: return <DirectionsBus sx={sx} />       // kFerryExit
    case 39: return <Elevator sx={{ fontSize: 16, color: '#00bcd4' }} />     // kElevatorEnter
    case 40: return <Stairs sx={{ fontSize: 16, color: '#ff9800' }} />       // kStepsEnter
    case 41: return <Stairs sx={{ fontSize: 16, color: '#ab47bc' }} />       // kEscalatorEnter
    case 42: return <MeetingRoom sx={{ fontSize: 16, color: '#4caf50' }} />  // kBuildingEnter
    case 43: return <MeetingRoom sx={{ fontSize: 16, color: '#ef5350', transform: 'scaleX(-1)' }} /> // kBuildingExit
    default: return <Straight sx={sx} />
  }
}

function ManeuverList({ maneuvers, color = 'text.secondary' }) {
  const [expanded, setExpanded] = useState(false)
  if (!maneuvers || maneuvers.length === 0) return null
  return (
    <Box sx={{ mt: 1 }}>
      <Box
        onClick={(e) => { e.stopPropagation(); setExpanded(!expanded) }}
        sx={{ display: 'flex', alignItems: 'center', gap: 0.5, cursor: 'pointer', '&:hover': { opacity: 0.8 } }}
      >
        <Route sx={{ fontSize: 14, color }} />
        <Typography variant="caption" sx={{ fontSize: 10, color, fontWeight: 600 }}>
          {maneuvers.length} instructions
        </Typography>
        {expanded ? <ExpandLess sx={{ fontSize: 14, color }} /> : <ExpandMore sx={{ fontSize: 14, color }} />}
      </Box>
      <Collapse in={expanded}>
        <Box sx={{ mt: 0.5, ml: 0.5, borderLeft: '2px solid', borderColor: 'divider', pl: 1.5 }}>
          {maneuvers.map((m, i) => (
            <Box key={i} sx={{ display: 'flex', alignItems: 'flex-start', gap: 1, py: 0.4 }}>
              {maneuverIcon(m.type)}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Typography variant="caption" sx={{ fontSize: 11, lineHeight: 1.3, display: 'block' }}>
                  {m.instruction}
                </Typography>
                {m.distance > 0 && (
                  <Typography variant="caption" sx={{ fontSize: 10, color: 'text.disabled' }}>
                    {m.distance >= 1000 ? `${(m.distance / 1000).toFixed(1)} km` : `${m.distance} m`}
                    {m.duration > 0 && ` · ${Math.ceil(m.duration / 60)} min`}
                  </Typography>
                )}
              </Box>
            </Box>
          ))}
        </Box>
      </Collapse>
    </Box>
  )
}

function WalkCard({ journey, selected, onSelect }) {
  const { t } = useI18n()
  const distKm = (journey.distance / 1000).toFixed(1)

  return (
    <Card
      sx={{
        mb: 1.5,
        border: '1px solid',
        borderColor: selected ? 'rgba(255, 184, 0, 0.4)' : 'rgba(255, 184, 0, 0.15)',
        bgcolor: selected ? 'rgba(255, 184, 0, 0.08)' : 'rgba(255, 184, 0, 0.04)',
        boxShadow: selected ? '0 0 24px rgba(255, 184, 0, 0.1), inset 0 1px 0 rgba(255,255,255,0.05)' : 'inset 0 1px 0 rgba(255,255,255,0.03)',
        '&:hover': {
          borderColor: selected ? 'rgba(255, 184, 0, 0.5)' : 'rgba(255, 184, 0, 0.3)',
          bgcolor: selected ? 'rgba(255, 184, 0, 0.1)' : 'rgba(255, 184, 0, 0.06)',
          transform: 'translateY(-1px)',
        },
        transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
        cursor: 'pointer',
      }}
      elevation={0}
    >
      <CardActionArea onClick={onSelect}>
        <CardContent sx={{ py: 1.5, px: 2.5 }}>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
            <Box sx={{
              width: 36, height: 36, borderRadius: '50%',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              bgcolor: selected ? 'rgba(255, 184, 0, 0.2)' : 'rgba(255, 184, 0, 0.12)',
              border: '1px solid',
              borderColor: selected ? 'rgba(255, 184, 0, 0.4)' : 'rgba(255, 184, 0, 0.25)',
            }}>
              <DirectionsWalk sx={{ fontSize: 20, color: '#ffb800' }} />
            </Box>
            <Box sx={{ flex: 1, minWidth: 0 }}>
              <Typography variant="body2" fontWeight={600} sx={{ fontFamily: '"Syne", sans-serif' }}>
                {t('walkJourney')}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                {t('walkDistance')}: {distKm} km
              </Typography>
            </Box>
            <Box sx={{ textAlign: 'right', flexShrink: 0 }}>
              <Typography variant="body1" fontWeight={800} lineHeight={1.2} sx={{ fontFamily: '"Syne", sans-serif', color: '#ffb800' }}>
                {formatDuration(journey.duration)}
              </Typography>
            </Box>
          </Box>
          <ManeuverList maneuvers={journey.maneuvers} color="#ffb800" />
        </CardContent>
      </CardActionArea>
    </Card>
  )
}

function BikeCard({ journey, selected, onSelect }) {
  const { t } = useI18n()
  const distKm = (journey.distance / 1000).toFixed(1)
  const bikeColors = { city: '#4caf50', ebike: '#00bcd4', road: '#ff9800' }
  const bikeLabels = { city: t('bikeCity'), ebike: t('bikeEbike'), road: t('bikeRoad') }
  const color = bikeColors[journey.type] || '#4caf50'

  return (
    <Card
      sx={{
        border: '1px solid',
        borderColor: selected ? `${color}66` : `${color}26`,
        bgcolor: selected ? `${color}14` : `${color}0a`,
        boxShadow: selected ? `0 0 24px ${color}1a, inset 0 1px 0 rgba(255,255,255,0.05)` : 'inset 0 1px 0 rgba(255,255,255,0.03)',
        '&:hover': {
          borderColor: selected ? `${color}80` : `${color}4d`,
          bgcolor: selected ? `${color}1a` : `${color}0f`,
          transform: 'translateY(-1px)',
        },
        transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
        cursor: 'pointer',
      }}
      elevation={0}
    >
      <CardActionArea onClick={onSelect}>
        <CardContent sx={{ py: 1.5, px: 2.5 }}>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
            <Box sx={{
              width: 36, height: 36, borderRadius: '50%',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              bgcolor: selected ? `${color}33` : `${color}1f`,
              border: '1px solid',
              borderColor: selected ? `${color}66` : `${color}40`,
            }}>
              <DirectionsBike sx={{ fontSize: 20, color }} />
            </Box>
            <Box sx={{ flex: 1, minWidth: 0 }}>
              <Typography variant="body2" fontWeight={600} sx={{ fontFamily: '"Syne", sans-serif' }}>
                {bikeLabels[journey.type] || journey.type}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                {distKm} km · ↑{journey.elevation_gain}m ↓{journey.elevation_loss}m
              </Typography>
            </Box>
            <Box sx={{ textAlign: 'right', flexShrink: 0 }}>
              <Typography variant="body1" fontWeight={800} lineHeight={1.2} sx={{ fontFamily: '"Syne", sans-serif', color }}>
                {formatDuration(journey.duration)}
              </Typography>
            </Box>
          </Box>
          <ManeuverList maneuvers={journey.maneuvers} color={color} />
        </CardContent>
      </CardActionArea>
    </Card>
  )
}

function CarCard({ journey }) {
  const { t } = useI18n()
  const distKm = (journey.distance / 1000).toFixed(1)
  const color = '#42a5f5'

  return (
    <Card
      sx={{
        border: '1px solid',
        borderColor: `${color}66`,
        bgcolor: `${color}14`,
        boxShadow: `0 0 24px ${color}1a, inset 0 1px 0 rgba(255,255,255,0.05)`,
        transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
      }}
      elevation={0}
    >
      <CardContent sx={{ py: 1.5, px: 2.5 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
          <Box sx={{
            width: 36, height: 36, borderRadius: '50%',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            bgcolor: `${color}33`,
            border: '1px solid',
            borderColor: `${color}66`,
          }}>
            <DirectionsCar sx={{ fontSize: 20, color }} />
          </Box>
          <Box sx={{ flex: 1, minWidth: 0 }}>
            <Typography variant="body2" fontWeight={600} sx={{ fontFamily: '"Syne", sans-serif' }}>
              {t('carJourney')}
            </Typography>
            <Typography variant="caption" color="text.secondary">
              {distKm} km
            </Typography>
          </Box>
          <Box sx={{ textAlign: 'right', flexShrink: 0 }}>
            <Typography variant="body1" fontWeight={800} lineHeight={1.2} sx={{ fontFamily: '"Syne", sans-serif', color }}>
              {formatDuration(journey.duration)}
            </Typography>
          </Box>
        </Box>
        <ManeuverList maneuvers={journey.maneuvers} color={color} />
      </CardContent>
    </Card>
  )
}

// --- Settings panel ---

function SettingsPanel({ status, onReload }) {
  const { t } = useI18n()
  const [reloading, setReloading] = useState(false)
  const [reloadMsg, setReloadMsg] = useState(null)

  const gtfs = status?.gtfs
  const raptor = status?.raptor

  const handleReload = async () => {
    setReloading(true); setReloadMsg(null)
    try {
      const res = await fetch('/api/reload', { method: 'POST' })
      const data = await res.json()
      if (data.error) {
        setReloadMsg({ severity: 'error', text: data.error.message })
      } else {
        setReloadMsg({ severity: 'success', text: t('reloadSuccess') })
        onReload()
      }
    } catch (err) {
      setReloadMsg({ severity: 'error', text: err.message })
    } finally { setReloading(false) }
  }

  const items = [
    { icon: <Route fontSize="small" />, label: t('routes'), value: gtfs?.routes },
    { icon: <Place fontSize="small" />, label: t('stopsLabel'), value: gtfs?.stops },
    { icon: <DirectionsBus fontSize="small" />, label: t('trips'), value: gtfs?.trips },
    { icon: <Timer fontSize="small" />, label: t('schedules'), value: gtfs?.stop_times },
    { icon: <MultipleStop fontSize="small" />, label: t('transfersLabel'), value: gtfs?.transfers },
    { icon: <CalendarMonth fontSize="small" />, label: t('calendars'), value: gtfs?.calendars },
    { icon: <Storage fontSize="small" />, label: t('calendarDates'), value: gtfs?.calendar_dates },
    { icon: <Search fontSize="small" />, label: t('agencies'), value: gtfs?.agencies },
  ]

  const raptorItems = [
    { icon: <Route fontSize="small" />, label: t('patterns'), value: raptor?.patterns },
    { icon: <DirectionsBus fontSize="small" />, label: t('services'), value: raptor?.services },
  ]

  if (!status) return (
    <Box sx={{ p: 4, textAlign: 'center' }}>
      <CircularProgress size={28} sx={{ color: '#00e5ff' }} />
      <Typography variant="body2" color="text.secondary" sx={{ mt: 1.5 }}>{t('loadingStatus')}</Typography>
    </Box>
  )

  return (
    <Box sx={{ overflow: 'auto', flex: 1 }}>
      <Box sx={{ px: 2.5, pt: 2, pb: 0.5 }}>
        <Typography variant="overline" color="primary.main" fontWeight={700} letterSpacing={2} fontSize={10}>
          {t('gtfsData')}
        </Typography>
      </Box>
      {items.map((item, i) => (
        <Box key={i} sx={{
          display: 'flex', alignItems: 'center', gap: 1.5, px: 2.5, py: 0.8,
          '&:hover': { bgcolor: 'rgba(255,255,255,0.02)' },
          transition: 'background 0.15s',
        }}>
          <Box sx={{ color: 'primary.main', display: 'flex', opacity: 0.7 }}>{item.icon}</Box>
          <Typography variant="body2" sx={{ flex: 1, color: 'text.secondary' }}>{item.label}</Typography>
          <Typography variant="body2" fontWeight={700}
            sx={{ fontFamily: '"Syne", sans-serif' }}>{item.value?.toLocaleString() ?? '—'}</Typography>
        </Box>
      ))}

      <Divider sx={{ my: 1.5, mx: 2, borderColor: 'rgba(255,255,255,0.04)' }} />

      <Box sx={{ px: 2.5, pb: 0.5 }}>
        <Typography variant="overline" color="secondary.main" fontWeight={700} letterSpacing={2} fontSize={10}>
          {t('raptorIndex')}
        </Typography>
      </Box>
      {raptorItems.map((item, i) => (
        <Box key={i} sx={{
          display: 'flex', alignItems: 'center', gap: 1.5, px: 2.5, py: 0.8,
          '&:hover': { bgcolor: 'rgba(255,255,255,0.02)' },
          transition: 'background 0.15s',
        }}>
          <Box sx={{ color: 'secondary.main', display: 'flex', opacity: 0.7 }}>{item.icon}</Box>
          <Typography variant="body2" sx={{ flex: 1, color: 'text.secondary' }}>{item.label}</Typography>
          <Typography variant="body2" fontWeight={700}
            sx={{ fontFamily: '"Syne", sans-serif' }}>{item.value?.toLocaleString() ?? '—'}</Typography>
        </Box>
      ))}

      <Divider sx={{ my: 1.5, mx: 2, borderColor: 'rgba(255,255,255,0.04)' }} />

      <Box sx={{ px: 2.5, pb: 1 }}>
        <Typography variant="overline" color="text.secondary" fontWeight={700} letterSpacing={2} fontSize={10}>
          {t('lastLoaded')}
        </Typography>
        <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
          {status.loaded_at
            ? new Date(status.loaded_at).toLocaleString(undefined, { dateStyle: 'full', timeStyle: 'medium' })
            : '—'}
        </Typography>
        <Chip label={status.status || '—'} size="small" variant="outlined"
          sx={{ mt: 1, fontWeight: 600, textTransform: 'uppercase', fontSize: 11,
            color: '#00e676', borderColor: 'rgba(0, 230, 118, 0.3)', bgcolor: 'rgba(0, 230, 118, 0.06)',
            fontFamily: '"Syne", sans-serif' }} />
      </Box>

      <Divider sx={{ my: 1.5, mx: 2, borderColor: 'rgba(255,255,255,0.04)' }} />

      <Box sx={{ px: 2.5, pb: 2 }}>
        {reloadMsg && (
          <Alert severity={reloadMsg.severity} sx={{ mb: 1.5, borderRadius: 2 }}>{reloadMsg.text}</Alert>
        )}
        <Button variant="contained" fullWidth onClick={handleReload} disabled={reloading}
          startIcon={reloading ? <CircularProgress size={18} color="inherit" /> : <Storage />}
          sx={{
            py: 1.2,
            bgcolor: 'rgba(0, 229, 255, 0.1)',
            color: '#00e5ff',
            border: '1px solid rgba(0, 229, 255, 0.25)',
            boxShadow: 'none',
            '&:hover': {
              bgcolor: 'rgba(0, 229, 255, 0.18)',
              boxShadow: '0 0 20px rgba(0, 229, 255, 0.15)',
            },
          }}>
          {reloading ? t('reloading') : t('reloadGtfs')}
        </Button>
      </Box>

      <Divider sx={{ my: 1, mx: 2, borderColor: 'rgba(255,255,255,0.04)' }} />

      <Box sx={{ px: 2.5, py: 1.5 }}>
        <Typography variant="overline" color="text.secondary" fontWeight={700} letterSpacing={2} fontSize={10}>
          {t('about')}
        </Typography>
        <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>{t('aboutDesc')}</Typography>
        <Typography variant="caption" sx={{ color: 'text.disabled', opacity: 0.5 }}>{t('aboutTech')}</Typography>
      </Box>
    </Box>
  )
}

// --- Swagger panel ---

function SwaggerPanel() {
  return (
    <Box sx={{
      flex: 1, overflow: 'auto',
      '& .swagger-ui': {
        fontFamily: '"Figtree", sans-serif',
      },
      '& .swagger-ui .topbar': { display: 'none' },
      '& .swagger-ui .info': { margin: '12px 0' },
      '& .swagger-ui .info .title': {
        fontFamily: '"Syne", sans-serif',
        color: '#e8e6f0',
      },
      '& .swagger-ui .info p, & .swagger-ui .info li': { color: '#8b89a0' },
      '& .swagger-ui .scheme-container': { background: 'transparent', boxShadow: 'none', padding: 0 },
      '& .swagger-ui .opblock-tag': { color: '#e8e6f0', borderColor: 'rgba(255,255,255,0.06)' },
      '& .swagger-ui .opblock': { borderColor: 'rgba(255,255,255,0.06)', background: 'rgba(255,255,255,0.02)' },
      '& .swagger-ui .opblock .opblock-summary-method': { borderRadius: '6px', fontFamily: '"Syne", sans-serif' },
      '& .swagger-ui .opblock .opblock-summary-description': { color: '#8b89a0' },
      '& .swagger-ui .opblock .opblock-summary-path': { color: '#e8e6f0' },
      '& .swagger-ui .opblock-body pre': { background: 'rgba(0,0,0,0.3)', color: '#e8e6f0' },
      '& .swagger-ui .model-box, & .swagger-ui .model': { color: '#8b89a0' },
      '& .swagger-ui table thead tr th': { color: '#8b89a0', borderColor: 'rgba(255,255,255,0.06)' },
      '& .swagger-ui table tbody tr td': { color: '#e8e6f0', borderColor: 'rgba(255,255,255,0.06)' },
      '& .swagger-ui .parameter__name': { color: '#00e5ff' },
      '& .swagger-ui .parameter__type': { color: '#8b89a0' },
      '& .swagger-ui .responses-inner h4, & .swagger-ui .responses-inner h5': { color: '#e8e6f0' },
      '& .swagger-ui .response-col_status': { color: '#00e676' },
      '& .swagger-ui .btn': { borderColor: 'rgba(255,255,255,0.1)', color: '#8b89a0' },
      '& .swagger-ui select': { background: 'rgba(20,20,35,0.8)', color: '#e8e6f0', borderColor: 'rgba(255,255,255,0.1)' },
      '& .swagger-ui input[type=text]': { background: 'rgba(20,20,35,0.8)', color: '#e8e6f0', borderColor: 'rgba(255,255,255,0.1)' },
      '& .swagger-ui .opblock-tag:hover': { color: '#00e5ff' },
      '& .swagger-ui .opblock.opblock-get .opblock-summary-method': { bgcolor: '#00e5ff', color: '#0a0a12' },
      '& .swagger-ui .opblock.opblock-post .opblock-summary-method': { bgcolor: '#ffb800', color: '#0a0a12' },
    }}>
      <SwaggerUI url="/api-docs/openapi.json" docExpansion="list" defaultModelsExpandDepth={-1} />
    </Box>
  )
}

// --- Metrics panel ---

function parsePrometheus(text) {
  const metrics = {}
  for (const line of text.split('\n')) {
    if (line.startsWith('#') || !line.trim()) continue
    const [name, value] = line.split(' ')
    if (name && value !== undefined) metrics[name] = parseFloat(value)
  }
  return metrics
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`
  return `${(bytes / 1073741824).toFixed(2)} GB`
}

function formatUptime(seconds) {
  const d = Math.floor(seconds / 86400)
  const h = Math.floor((seconds % 86400) / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const s = Math.floor(seconds % 60)
  if (d > 0) return `${d}d ${h}h ${m}m`
  if (h > 0) return `${h}h ${m}m ${s}s`
  return `${m}m ${s}s`
}

function MetricsPanel() {
  const { t } = useI18n()
  const [metrics, setMetrics] = useState(null)

  useEffect(() => {
    const load = () => fetch('/api/metrics').then(r => r.text()).then(t => setMetrics(parsePrometheus(t))).catch(() => {})
    load()
    const interval = setInterval(load, 5000)
    return () => clearInterval(interval)
  }, [])

  if (!metrics) return (
    <Box sx={{ p: 4, textAlign: 'center' }}>
      <CircularProgress size={28} sx={{ color: '#00e5ff' }} />
      <Typography variant="body2" color="text.secondary" sx={{ mt: 1.5 }}>{t('metricsLoadingMetrics')}</Typography>
    </Box>
  )

  const processItems = [
    { icon: <Speed fontSize="small" />, label: t('metricsCpu'), value: `${metrics.process_cpu_seconds_total?.toFixed(2)}s` },
    { icon: <Memory fontSize="small" />, label: t('metricsMemoryRss'), value: formatBytes(metrics.process_resident_memory_bytes || 0) },
    { icon: <Memory fontSize="small" />, label: t('metricsMemoryVirtual'), value: formatBytes(metrics.process_virtual_memory_bytes || 0) },
    { icon: <Dns fontSize="small" />, label: t('metricsOpenFds'), value: metrics.process_open_fds?.toLocaleString() },
    { icon: <Dns fontSize="small" />, label: t('metricsThreads'), value: metrics.process_threads?.toLocaleString() },
    { icon: <Timer fontSize="small" />, label: t('metricsUptime'), value: formatUptime(metrics.process_uptime_seconds || 0) },
  ]

  const httpItems = [
    { icon: <Http fontSize="small" />, label: t('metricsRequests'), value: metrics.glove_http_requests_total?.toLocaleString() },
    { icon: <Http fontSize="small" />, label: t('metricsErrors'), value: metrics.glove_http_errors_total?.toLocaleString() },
  ]

  const renderSection = (title, items, color) => (
    <>
      <Box sx={{ px: 2.5, pt: 2, pb: 0.5 }}>
        <Typography variant="overline" sx={{ color }} fontWeight={700} letterSpacing={2} fontSize={10}>
          {title}
        </Typography>
      </Box>
      {items.map((item, i) => (
        <Box key={i} sx={{
          display: 'flex', alignItems: 'center', gap: 1.5, px: 2.5, py: 0.8,
          '&:hover': { bgcolor: 'rgba(255,255,255,0.02)' },
          transition: 'background 0.15s',
        }}>
          {item.icon && <Box sx={{ color, display: 'flex', opacity: 0.7 }}>{item.icon}</Box>}
          {!item.icon && <Box sx={{ width: 20 }} />}
          <Typography variant="body2" sx={{ flex: 1, color: 'text.secondary' }}>{item.label}</Typography>
          <Typography variant="body2" fontWeight={700}
            sx={{ fontFamily: '"Syne", sans-serif' }}>{item.value?.toLocaleString() ?? '—'}</Typography>
        </Box>
      ))}
      <Divider sx={{ my: 1.5, mx: 2, borderColor: 'rgba(255,255,255,0.04)' }} />
    </>
  )

  return (
    <Box sx={{ overflow: 'auto', flex: 1 }}>
      {renderSection(t('metricsProcess'), processItems, '#00e5ff')}
      {renderSection(t('metricsHttp'), httpItems, '#ffb800')}
    </Box>
  )
}

// --- Main App ---

export default function App() {
  const { t, lang, setLang } = useI18n()

  const [from, setFrom] = useState(null)
  const [to, setTo] = useState(null)
  const [departDate, setDepartDate] = useState(null)  // dayjs object or null
  const [departTime, setDepartTime] = useState(null)  // dayjs object or null
  const [showOptions, setShowOptions] = useState(false)
  const isNow = !departDate && !departTime
  const [journeys, setJourneys] = useState(null)
  const [walkJourney, setWalkJourney] = useState(null)
  const [bikeJourneys, setBikeJourneys] = useState(null)
  const [carJourney, setCarJourney] = useState(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(null)
  const [status, setStatus] = useState(null)
  const [selectedJourney, setSelectedJourney] = useState(0)
  const [ptTime, setPtTime] = useState(null)
  const [walkTime, setWalkTime] = useState(null)
  const [bikeTime, setBikeTime] = useState(null)
  const [carTime, setCarTime] = useState(null)
  const [resultTab, setResultTab] = useState('pt')
  const [view, setView] = useState('search')
  const [walkingSpeed, setWalkingSpeed] = useState(() => {
    const saved = localStorage.getItem('glove_walking_speed')
    return saved ? parseFloat(saved) : 5
  })
  const handleWalkingSpeedChange = (v) => {
    setWalkingSpeed(v)
    localStorage.setItem('glove_walking_speed', String(v))
  }

  const defaultModes = { metro: true, rail: true, bus: true, tramway: true, walk: true, bike: true, car: true }
  const [modes, setModes] = useState(() => {
    const saved = localStorage.getItem('glove_modes')
    return saved ? { ...defaultModes, ...JSON.parse(saved) } : defaultModes
  })
  const toggleMode = (mode) => {
    setModes(prev => {
      const next = { ...prev, [mode]: !prev[mode] }
      localStorage.setItem('glove_modes', JSON.stringify(next))
      return next
    })
  }

  const refreshStatus = () => {
    fetch('/api/status').then(r => r.json()).then(setStatus).catch(() => {})
  }
  useEffect(() => { refreshStatus() }, [])

  const clearResults = () => { setJourneys(null); setWalkJourney(null); setBikeJourneys(null); setCarJourney(null); setSelectedJourney(0); setResultTab('pt'); setError(null); setPtTime(null); setWalkTime(null); setBikeTime(null); setCarTime(null) }
  const handleFromChange = (v) => { setFrom(v); clearResults() }
  const handleToChange = (v) => { setTo(v); clearResults() }
  const swap = () => { setFrom(to); setTo(from); clearResults() }

  const search = async (e) => {
    e.preventDefault()
    if (!from || !to) return
    saveRecentPlace(from)
    saveRecentPlace(to)
    setLoading(true); setError(null); setJourneys(null); setWalkJourney(null); setBikeJourneys(null); setCarJourney(null); setSelectedJourney(0); setResultTab('pt'); setPtTime(null); setWalkTime(null); setBikeTime(null); setCarTime(null)
    try {
      let effectiveDatetime
      if (departDate || departTime) {
        const d = departDate ? departDate.format('YYYY-MM-DD') : defaultDatetime().slice(0, 10)
        const t2 = departTime ? departTime.format('HH:mm') : '00:00'
        effectiveDatetime = `${d}T${t2}`
      } else {
        effectiveDatetime = defaultDatetime()
      }
      const fromCoord = from.stop_point?.coord || from.coord
      const toCoord = to.stop_point?.coord || to.coord
      const ptFrom = fromCoord ? `${fromCoord.lon};${fromCoord.lat}` : from.id
      const ptTo = toCoord ? `${toCoord.lon};${toCoord.lat}` : to.id
      const ptParams = new URLSearchParams({ from: ptFrom, to: ptTo, datetime: toApiDatetime(effectiveDatetime) })
      if (walkingSpeed !== 5) ptParams.set('walking_speed', String(walkingSpeed))
      const forbidden = ['metro', 'rail', 'bus', 'tramway'].filter(m => !modes[m])
      if (forbidden.length > 0) ptParams.set('forbidden_modes', forbidden.join(','))
      const ptT0 = performance.now()
      const ptFetch = fetch(`/api/journeys/public_transport?${ptParams}`)
        .then(r => r.json())
        .then(data => { setPtTime(Math.round(performance.now() - ptT0)); return data })

      // Walk, bike and car requests use lon;lat coordinates
      let walkFetch = null
      let bikeFetch = null
      let carFetch = null
      if (fromCoord && toCoord) {
        const coordParams = new URLSearchParams({
          from: `${fromCoord.lon};${fromCoord.lat}`,
          to: `${toCoord.lon};${toCoord.lat}`,
        })
        if (modes.walk) {
          const walkParams = new URLSearchParams(coordParams)
          if (walkingSpeed !== 5) walkParams.set('walking_speed', String(walkingSpeed))
          const walkT0 = performance.now()
          walkFetch = fetch(`/api/journeys/walk?${walkParams}`)
            .then(r => r.ok ? r.json() : null)
            .then(data => { setWalkTime(Math.round(performance.now() - walkT0)); return data })
            .catch(() => null)
        }
        if (modes.bike) {
          const bikeT0 = performance.now()
          bikeFetch = fetch(`/api/journeys/bike?${coordParams}`)
            .then(r => r.ok ? r.json() : null)
            .then(data => { setBikeTime(Math.round(performance.now() - bikeT0)); return data })
            .catch(() => null)
        }
        if (modes.car) {
          const carT0 = performance.now()
          carFetch = fetch(`/api/journeys/car?${coordParams}`)
            .then(r => r.ok ? r.json() : null)
            .then(data => { setCarTime(Math.round(performance.now() - carT0)); return data })
            .catch(() => null)
        }
      }

      const [ptData, walkData, bikeData, carData] = await Promise.all([ptFetch, walkFetch, bikeFetch, carFetch])

      if (ptData.error) setError(ptData.error.message)
      else setJourneys(ptData.journeys)

      if (walkData?.journeys?.[0]) setWalkJourney(walkData.journeys[0])
      if (bikeData?.journeys?.length > 0) setBikeJourneys(bikeData.journeys)
      if (carData?.journeys?.[0]) setCarJourney(carData.journeys[0])
    } catch (err) {
      setError(err.message)
    } finally { setLoading(false) }
  }

  const isWalkSelected = resultTab === 'walk'
  const isBikeSelected = resultTab === 'bike'
  const isCarSelected = resultTab === 'car'
  const selectedBike = isBikeSelected ? bikeJourneys?.[selectedJourney] || bikeJourneys?.[0] : null
  const selectedJ = (isWalkSelected || isBikeSelected || isCarSelected) ? null : journeys?.[selectedJourney]
  const mapData = selectedJ ? extractMapData(selectedJ) : { lines: [], stopPoints: [], labeledStops: [] }
  const walkCoords = (isWalkSelected && walkJourney?.shape) ? decodePolyline(walkJourney.shape) : []
  const bikeCoords = (isBikeSelected && selectedBike?.shape) ? decodePolyline(selectedBike.shape) : []
  const bikeElevSegs = (isBikeSelected && bikeCoords.length >= 2 && selectedBike?.heights)
    ? elevationSegments(bikeCoords, selectedBike.heights) : []
  const carCoords = (isCarSelected && carJourney?.shape) ? decodePolyline(carJourney.shape) : []
  const allCoords = isWalkSelected ? walkCoords : isBikeSelected ? bikeCoords : isCarSelected ? carCoords : mapData.lines.flatMap(l => l.coords)
  const hasResults = allCoords.length >= 2
  const fromPos = from?.coord ? [from.coord.lat, from.coord.lon] : null
  const toPos = to?.coord ? [to.coord.lat, to.coord.lon] : null
  const fitBounds = hasResults ? allCoords : (fromPos && toPos) ? [fromPos, toPos] : null
  const flyTo = !fitBounds && !hasResults ? (toPos || fromPos) : null

  const toggleLang = () => setLang(lang === 'fr' ? 'en' : 'fr')

  const resultsText = journeys
    ? journeys.length === 1
      ? t('journeyFound', { count: 1 })
      : t('journeysFound', { count: journeys.length })
    : null

  return (
    <LocalizationProvider dateAdapter={AdapterDayjs} adapterLocale={lang}>
    <Box sx={{ display: 'flex', height: '100vh', overflow: 'hidden', bgcolor: '#0a0a12' }}>
      {/* Left panel — floating glass sidebar */}
      <Box sx={{
        width: 460, minWidth: 460,
        display: 'flex', flexDirection: 'column',
        zIndex: 1200, overflow: 'hidden',
        position: 'relative',
        bgcolor: 'rgba(10, 10, 18, 0.92)',
        backdropFilter: 'blur(24px)',
        borderRight: '1px solid rgba(255, 255, 255, 0.04)',
      }}>
        {/* Header */}
        <Box sx={{
          px: 3, py: 2,
          background: 'linear-gradient(135deg, rgba(0, 229, 255, 0.08) 0%, rgba(255, 184, 0, 0.05) 100%)',
          borderBottom: '1px solid rgba(255, 255, 255, 0.06)',
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          flexShrink: 0,
          position: 'relative',
          overflow: 'hidden',
        }}>
          {/* Subtle gradient accent line */}
          <Box sx={{
            position: 'absolute', bottom: 0, left: 0, right: 0, height: '1px',
            background: 'linear-gradient(90deg, #00e5ff 0%, transparent 40%, transparent 60%, #ffb800 100%)',
            opacity: 0.5,
          }} />

          <Stack direction="row" alignItems="center" spacing={1.5} sx={{ cursor: 'pointer' }}
            onClick={() => setView('search')}>
            <Box sx={{
              width: 32, height: 32, borderRadius: '8px', display: 'flex', alignItems: 'center', justifyContent: 'center',
              background: 'linear-gradient(135deg, #00e5ff 0%, #00b8d4 100%)',
              boxShadow: '0 0 16px rgba(0, 229, 255, 0.25)',
            }}>
              <NearMe sx={{ fontSize: 18, color: '#0a0a12' }} />
            </Box>
            <Typography variant="h6" sx={{
              fontFamily: '"Syne", sans-serif',
              fontWeight: 800,
              fontSize: '1.25rem',
              letterSpacing: '-0.03em',
              background: 'linear-gradient(135deg, #e8e6f0 0%, #8b89a0 100%)',
              backgroundClip: 'text',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
            }}>
              Glove
            </Typography>
          </Stack>

          <Stack direction="row" spacing={0.5}>
            <Tooltip title={lang === 'fr' ? 'English' : 'Français'}>
              <IconButton onClick={toggleLang} size="small"
                sx={{
                  color: 'text.secondary',
                  '&:hover': { color: '#00e5ff', bgcolor: 'rgba(0, 229, 255, 0.08)' },
                  transition: 'all 0.2s',
                }}>
                <Language fontSize="small" />
              </IconButton>
            </Tooltip>
            <Tooltip title="API">
              <IconButton
                onClick={() => setView(view === 'swagger' ? 'search' : 'swagger')}
                size="small"
                sx={{
                  color: view === 'swagger' ? '#00e5ff' : 'text.secondary',
                  '&:hover': { color: '#00e5ff', bgcolor: 'rgba(0, 229, 255, 0.08)' },
                  transition: 'all 0.2s',
                }}>
                {view === 'swagger' ? <Close fontSize="small" /> : <Api fontSize="small" />}
              </IconButton>
            </Tooltip>
            <Tooltip title={view === 'metrics' ? t('search') : t('metrics')}>
              <IconButton
                onClick={() => setView(view === 'metrics' ? 'search' : 'metrics')}
                size="small"
                sx={{
                  color: view === 'metrics' ? '#00e5ff' : 'text.secondary',
                  '&:hover': { color: '#00e5ff', bgcolor: 'rgba(0, 229, 255, 0.08)' },
                  transition: 'all 0.2s',
                }}>
                {view === 'metrics' ? <Close fontSize="small" /> : <MonitorHeart fontSize="small" />}
              </IconButton>
            </Tooltip>
            <Tooltip title={view === 'settings' ? t('search') : t('settings')}>
              <IconButton
                onClick={() => setView(view === 'settings' ? 'search' : 'settings')}
                size="small"
                sx={{
                  color: view === 'settings' ? '#00e5ff' : 'text.secondary',
                  '&:hover': { color: '#00e5ff', bgcolor: 'rgba(0, 229, 255, 0.08)' },
                  transition: 'all 0.2s',
                }}>
                {view === 'settings' ? <Close fontSize="small" /> : <Settings fontSize="small" />}
              </IconButton>
            </Tooltip>
          </Stack>
        </Box>

        {view === 'swagger' ? (
          <SwaggerPanel />
        ) : view === 'metrics' ? (
          <MetricsPanel />
        ) : view === 'settings' ? (
          <SettingsPanel status={status} onReload={refreshStatus} />
        ) : (
          <>
            {/* Search form */}
            <Box sx={{ p: 2.5, flexShrink: 0 }}>
              <Paper component="form" onSubmit={search}
                sx={{
                  p: 2.5,
                  bgcolor: 'rgba(20, 20, 35, 0.5)',
                  border: '1px solid rgba(255, 255, 255, 0.06)',
                  position: 'relative',
                  overflow: 'hidden',
                }}
                elevation={0}
              >
                {/* Decorative corner accent */}
                <Box sx={{
                  position: 'absolute', top: 0, left: 0, width: 40, height: 2,
                  background: 'linear-gradient(90deg, #00e5ff, transparent)',
                  opacity: 0.6,
                }} />
                <Box sx={{
                  position: 'absolute', top: 0, left: 0, width: 2, height: 40,
                  background: 'linear-gradient(180deg, #00e5ff, transparent)',
                  opacity: 0.6,
                }} />

                <Stack spacing={2}>
                  <Box sx={{ display: 'flex', gap: 1, alignItems: 'center' }}>
                    <Box sx={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 1.5 }}>
                      <PlaceAutocomplete label={t('departure')} value={from} onChange={handleFromChange}
                        icon={<Box sx={{
                          width: 10, height: 10, borderRadius: '50%', ml: 1,
                          bgcolor: '#0a0a12', border: '2px solid #00e676',
                          boxShadow: '0 0 8px rgba(0, 230, 118, 0.5), inset 0 0 0 1.5px #00e676',
                        }} />} />
                      <PlaceAutocomplete label={t('arrival')} value={to} onChange={handleToChange}
                        icon={<Box sx={{
                          width: 10, height: 10, borderRadius: '50%', ml: 1, position: 'relative',
                          background: 'linear-gradient(135deg, #ff5252, #d32f2f)',
                          boxShadow: '0 0 8px rgba(255, 82, 82, 0.5)',
                        }} />} />
                    </Box>
                    <Tooltip title={t('swap')}>
                      <IconButton onClick={swap} size="small"
                        sx={{
                          border: '1px solid rgba(255, 255, 255, 0.08)',
                          borderRadius: 2, p: 0.8,
                          color: 'text.secondary',
                          '&:hover': {
                            borderColor: 'rgba(0, 229, 255, 0.3)',
                            color: '#00e5ff',
                            bgcolor: 'rgba(0, 229, 255, 0.06)',
                          },
                          transition: 'all 0.2s',
                        }}>
                        <SwapVert fontSize="small" />
                      </IconButton>
                    </Tooltip>
                  </Box>

                  {/* Departure time: "Now" / "Later" chips + collapsible date/time pickers */}
                  <Box>
                    <Box sx={{ display: 'flex', gap: 1, alignItems: 'center' }}>
                      <Chip
                        icon={<AccessTime fontSize="small" />}
                        label={t('now')}
                        onClick={() => { setDepartDate(null); setDepartTime(null) }}
                        variant={isNow ? 'filled' : 'outlined'}
                        sx={{
                          fontWeight: 600, fontSize: 12,
                          ...(isNow ? {
                            bgcolor: 'rgba(0, 229, 255, 0.15)', color: '#00e5ff',
                            border: '1px solid rgba(0, 229, 255, 0.3)',
                          } : {
                            borderColor: 'rgba(255,255,255,0.12)', color: 'text.secondary',
                          }),
                        }}
                      />
                      <Chip
                        icon={<CalendarMonth fontSize="small" />}
                        label={t('later')}
                        onClick={() => { if (isNow) { setDepartDate(dayjs()); setDepartTime(dayjs()) } }}
                        variant={!isNow ? 'filled' : 'outlined'}
                        sx={{
                          fontWeight: 600, fontSize: 12,
                          ...(!isNow ? {
                            bgcolor: 'rgba(0, 229, 255, 0.15)', color: '#00e5ff',
                            border: '1px solid rgba(0, 229, 255, 0.3)',
                          } : {
                            borderColor: 'rgba(255,255,255,0.12)', color: 'text.secondary',
                          }),
                        }}
                      />
                    </Box>
                    <Collapse in={!isNow}>
                      <Box sx={{ display: 'flex', gap: 1, mt: 1.5 }}>
                        <DatePicker
                          value={departDate}
                          onChange={setDepartDate}
                          label={t('date')}
                          format="DD/MM/YYYY"
                          slotProps={{
                            textField: {
                              size: 'small',
                              sx: { flex: 1, '& .MuiInputBase-input': { fontSize: 12, py: 0.6 }, '& .MuiInputLabel-root': { fontSize: 12 }, '& .MuiInputAdornment-root': { ml: 0 } },
                            },
                            openPickerButton: { sx: { p: 0.3, '& svg': { fontSize: 18 } } },
                          }}
                        />
                        <TimePicker
                          value={departTime}
                          onChange={setDepartTime}
                          label={t('time')}
                          ampm={false}
                          slotProps={{
                            textField: {
                              size: 'small',
                              sx: { width: 105, '& .MuiInputBase-input': { fontSize: 12, py: 0.6 }, '& .MuiInputLabel-root': { fontSize: 12 }, '& .MuiInputAdornment-root': { ml: 0 } },
                            },
                            openPickerButton: { sx: { p: 0.3, '& svg': { fontSize: 18 } } },
                          }}
                        />
                      </Box>
                    </Collapse>
                  </Box>

                  {/* Collapsible options */}
                  <Box>
                    <Box
                      onClick={() => setShowOptions(v => !v)}
                      sx={{
                        display: 'flex', alignItems: 'center', gap: 0.5, cursor: 'pointer',
                        color: 'text.secondary', '&:hover': { color: '#00e5ff' },
                        transition: 'color 0.15s', userSelect: 'none',
                      }}
                    >
                      <Settings sx={{ fontSize: 14 }} />
                      <Typography variant="caption" sx={{ fontSize: 11, fontWeight: 600, letterSpacing: 0.5 }}>
                        {t('preferences')}
                      </Typography>
                      {showOptions ? <ExpandLess sx={{ fontSize: 16 }} /> : <ExpandMore sx={{ fontSize: 16 }} />}
                    </Box>
                    <Collapse in={showOptions}>
                      <Box sx={{ pt: 1.5, pb: 0.5 }}>
                        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                          <DirectionsWalk sx={{ fontSize: 16, color: 'text.secondary' }} />
                          <Typography variant="caption" sx={{ color: 'text.secondary', fontSize: 11 }}>
                            {t('walkingSpeed')}
                          </Typography>
                          <Typography variant="caption" fontWeight={700} sx={{ ml: 'auto', fontFamily: '"Syne", sans-serif', color: '#00e5ff' }}>
                            {walkingSpeed} {t('walkingSpeedUnit')}
                          </Typography>
                        </Box>
                        <Slider
                          value={walkingSpeed}
                          onChange={(_, v) => handleWalkingSpeedChange(v)}
                          min={2} max={10} step={0.5}
                          marks={[{ value: 2, label: '2' }, { value: 5, label: '5' }, { value: 10, label: '10' }]}
                          sx={{
                            color: '#00e5ff', mt: 0.5,
                            '& .MuiSlider-markLabel': { fontSize: 9, color: 'text.disabled' },
                            '& .MuiSlider-thumb': { width: 12, height: 12 },
                            '& .MuiSlider-rail': { opacity: 0.2 },
                          }}
                        />

                        <Typography variant="caption" sx={{ color: 'text.secondary', fontSize: 11, mt: 1, mb: 0.5, display: 'block' }}>
                          {t('transportModes')}
                        </Typography>
                        <Box sx={{ display: 'flex', flexWrap: 'wrap', gap: 0.25 }}>
                          {[
                            { key: 'metro', icon: <Subway sx={{ fontSize: 16 }} />, label: t('modeMetro') },
                            { key: 'rail', icon: <Train sx={{ fontSize: 16 }} />, label: t('modeRail') },
                            { key: 'tramway', icon: <Tram sx={{ fontSize: 16 }} />, label: t('modeTramway') },
                            { key: 'bus', icon: <DirectionsBus sx={{ fontSize: 16 }} />, label: t('modeBus') },
                            { key: 'walk', icon: <DirectionsWalk sx={{ fontSize: 16 }} />, label: t('modeWalk') },
                            { key: 'bike', icon: <DirectionsBike sx={{ fontSize: 16 }} />, label: t('modeBike') },
                            { key: 'car', icon: <DirectionsCar sx={{ fontSize: 16 }} />, label: t('modeCar') },
                          ].map(m => (
                            <Chip
                              key={m.key}
                              icon={m.icon}
                              label={m.label}
                              size="small"
                              onClick={() => toggleMode(m.key)}
                              variant={modes[m.key] ? 'filled' : 'outlined'}
                              sx={{
                                fontSize: 10,
                                height: 26,
                                bgcolor: modes[m.key] ? 'rgba(0, 229, 255, 0.15)' : 'transparent',
                                borderColor: modes[m.key] ? 'rgba(0, 229, 255, 0.4)' : 'rgba(255,255,255,0.12)',
                                color: modes[m.key] ? '#00e5ff' : 'text.disabled',
                                '& .MuiChip-icon': { color: modes[m.key] ? '#00e5ff' : 'text.disabled' },
                                '&:hover': { bgcolor: 'rgba(0, 229, 255, 0.1)' },
                              }}
                            />
                          ))}
                        </Box>
                      </Box>
                    </Collapse>
                  </Box>

                  <Button type="submit" variant="contained" fullWidth size="large"
                    disabled={loading || !from || !to}
                    startIcon={loading ? <CircularProgress size={18} color="inherit" /> : <Search />}
                    sx={{
                      py: 1.3,
                      bgcolor: '#00e5ff',
                      color: '#0a0a12',
                      fontWeight: 700,
                      '&:hover': {
                        bgcolor: '#00b8d4',
                        boxShadow: '0 0 24px rgba(0, 229, 255, 0.3)',
                      },
                      '&:disabled': {
                        bgcolor: 'rgba(0, 229, 255, 0.1)',
                        color: 'rgba(0, 229, 255, 0.3)',
                      },
                      animation: from && to ? 'glowPulse 3s ease-in-out infinite' : 'none',
                      transition: 'all 0.25s cubic-bezier(0.4, 0, 0.2, 1)',
                    }}>
                    {loading ? t('searching') : t('search')}
                  </Button>
                </Stack>
              </Paper>
            </Box>

            {/* Results */}
            <Box sx={{ flex: 1, overflow: 'auto', px: 2.5, pb: 2.5 }}>
              {error && <Alert severity="error" sx={{ mb: 1.5, borderRadius: 2 }}>{error}</Alert>}

              {journeys && journeys.length === 0 && !walkJourney && !bikeJourneys && !carJourney && (
                <Alert severity="info" sx={{ borderRadius: 2 }}>{t('noResults')}</Alert>
              )}

              {(journeys?.length > 0 || walkJourney || bikeJourneys || carJourney) && (
                <>
                  {/* Mode tabs */}
                  <Box sx={{ display: 'flex', gap: 0.75, mb: 1.5 }}>
                    {[
                      { key: 'pt', icon: <DirectionsBus sx={{ fontSize: 18 }} />, label: t('tabPublicTransport'),
                        duration: journeys?.[0]?.duration, disabled: false },
                      { key: 'walk', icon: <DirectionsWalk sx={{ fontSize: 18 }} />, label: t('tabWalk'),
                        duration: walkJourney?.duration, disabled: false },
                      { key: 'bike', icon: <DirectionsBike sx={{ fontSize: 18 }} />, label: t('tabBike'),
                        duration: bikeJourneys?.[0]?.duration, disabled: false },
                      { key: 'car', icon: <DirectionsCar sx={{ fontSize: 18 }} />, label: t('tabCar'),
                        duration: carJourney?.duration, disabled: false },
                    ].map(tab => {
                      const active = resultTab === tab.key
                      const hasData = tab.key === 'pt' ? journeys?.length > 0 : tab.key === 'walk' ? !!walkJourney : tab.key === 'bike' ? !!bikeJourneys : tab.key === 'car' ? !!carJourney : false
                      return (
                        <Tooltip key={tab.key} title={tab.disabled ? t('comingSoon') : ''} arrow>
                          <Box
                            component="button"
                            onClick={() => {
                              if (tab.disabled || !hasData) return
                              setResultTab(tab.key)
                              if (tab.key === 'pt' || tab.key === 'bike') setSelectedJourney(0)
                            }}
                            sx={{
                              flex: 1,
                              display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 0.25,
                              py: 1, px: 0.5,
                              border: '1px solid',
                              borderColor: active ? 'rgba(0, 229, 255, 0.4)' : 'rgba(255,255,255,0.08)',
                              borderRadius: 2,
                              bgcolor: active ? 'rgba(0, 229, 255, 0.1)' : 'transparent',
                              color: tab.disabled ? 'text.disabled' : active ? '#00e5ff' : 'text.secondary',
                              cursor: tab.disabled || !hasData ? 'default' : 'pointer',
                              opacity: tab.disabled ? 0.4 : !hasData ? 0.5 : 1,
                              transition: 'all 0.2s ease',
                              '&:hover': !tab.disabled && hasData ? {
                                borderColor: 'rgba(0, 229, 255, 0.3)',
                                bgcolor: 'rgba(0, 229, 255, 0.05)',
                              } : {},
                              fontFamily: '"Syne", sans-serif',
                            }}
                          >
                            {tab.icon}
                            <Typography sx={{ fontSize: '0.6rem', fontFamily: 'inherit', letterSpacing: '0.03em', lineHeight: 1 }}>
                              {tab.label}
                            </Typography>
                            <Typography sx={{ fontSize: '0.7rem', fontWeight: 700, fontFamily: 'inherit', lineHeight: 1, mt: 0.25 }}>
                              {tab.disabled ? '—' : tab.duration != null ? formatDuration(tab.duration) : '—'}
                            </Typography>
                          </Box>
                        </Tooltip>
                      )
                    })}
                  </Box>

                  {/* PT results */}
                  {resultTab === 'pt' && journeys?.length > 0 && (
                    <>
                      <Typography variant="caption" color="text.secondary"
                        sx={{ mb: 1.5, display: 'block', fontFamily: '"Syne", sans-serif', letterSpacing: '0.05em' }}>
                        {resultsText}
                      </Typography>
                      {journeys.map((j, i) => (
                        <JourneyCard key={i} journey={j}
                          selected={i === selectedJourney}
                          onSelect={() => setSelectedJourney(i)}
                          animDelay={i * 80} />
                      ))}
                    </>
                  )}

                  {/* Walk results */}
                  {resultTab === 'walk' && walkJourney && (
                    <WalkCard journey={walkJourney} selected={true} onSelect={() => {}} />
                  )}

                  {/* Bike results */}
                  {resultTab === 'bike' && bikeJourneys && (
                    <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1.5 }}>
                      {bikeJourneys.map((bj, i) => (
                        <BikeCard key={bj.type} journey={bj}
                          selected={i === selectedJourney}
                          onSelect={() => setSelectedJourney(i)} />
                      ))}
                    </Box>
                  )}

                  {/* Car results */}
                  {resultTab === 'car' && carJourney && (
                    <CarCard journey={carJourney} />
                  )}

                  {((resultTab === 'pt' && ptTime != null) || (resultTab === 'walk' && walkTime != null) || (resultTab === 'bike' && bikeTime != null) || (resultTab === 'car' && carTime != null)) && (
                    <Typography variant="caption"
                      sx={{ display: 'block', textAlign: 'right', mt: 0.5, color: 'text.disabled',
                        fontFamily: '"Syne", sans-serif', fontSize: '0.65rem', letterSpacing: '0.04em' }}>
                      {{ pt: ptTime, walk: walkTime, bike: bikeTime, car: carTime }[resultTab]} ms
                    </Typography>
                  )}
                </>
              )}
            </Box>
          </>
        )}
      </Box>

      {/* Map */}
      <Box sx={{ flex: 1, position: 'relative' }}>
        {/* Vignette overlay on map edges near sidebar */}
        <Box sx={{
          position: 'absolute', top: 0, left: 0, bottom: 0, width: 60, zIndex: 500,
          background: 'linear-gradient(90deg, rgba(10, 10, 18, 0.4) 0%, transparent 100%)',
          pointerEvents: 'none',
        }} />
        <MapContainer center={status?.map?.center || DEFAULT_CENTER} zoom={status?.map?.zoom || DEFAULT_ZOOM}
          minZoom={status?.map?.zoom || DEFAULT_ZOOM}
          maxBounds={status?.map?.bounds || [[48.1, 1.4], [49.3, 3.6]]}
          maxBoundsViscosity={1.0}
          style={{ height: '100%', width: '100%' }} zoomControl={false}>
          <TileLayer
            attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OSM</a> &copy; <a href="https://carto.com/">CARTO</a>'
            url="https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png"
          />

          {fitBounds && <FitBounds bounds={fitBounds} />}
          {flyTo && <FlyToPoint point={flyTo} />}

          {fromPos && toPos && !hasResults && (
            <Polyline positions={[fromPos, toPos]}
              pathOptions={{ color: '#56546a', weight: 2, opacity: 0.7, dashArray: '8, 8' }} />
          )}

          {mapData.lines.map((line, i) => (
            <Polyline key={`line-${i}`} positions={smoothLine(line.coords)}
              pathOptions={{
                color: line.color, weight: line.dashed ? 4 : 5, opacity: 0.9, lineCap: 'round', lineJoin: 'round',
                ...(line.dashed ? { dashArray: '8, 6' } : {}),
              }} />
          ))}

          {isWalkSelected && walkCoords.length >= 2 && (
            <Polyline positions={walkCoords}
              pathOptions={{ color: '#ffb800', weight: 4, opacity: 0.9, dashArray: '8, 6', lineCap: 'round', lineJoin: 'round' }} />
          )}

          {isBikeSelected && bikeElevSegs.length > 0 && bikeElevSegs.map((seg, i) => (
            <Polyline key={`bike-seg-${i}`} positions={seg.positions}
              pathOptions={{ color: seg.color, weight: 5, opacity: 0.9, lineCap: 'round', lineJoin: 'round' }} />
          ))}
          {isBikeSelected && bikeCoords.length >= 2 && bikeElevSegs.length === 0 && (
            <Polyline positions={bikeCoords}
              pathOptions={{ color: { city: '#4caf50', ebike: '#00bcd4', road: '#ff9800' }[selectedBike?.type] || '#4caf50', weight: 4, opacity: 0.9, lineCap: 'round', lineJoin: 'round' }} />
          )}

          {isCarSelected && carCoords.length >= 2 && (
            <Polyline positions={carCoords}
              pathOptions={{ color: '#42a5f5', weight: 4, opacity: 0.9, lineCap: 'round', lineJoin: 'round' }} />
          )}

          {mapData.stopPoints.map((sp, i) => (
            <CircleMarker key={`sp-${i}`} center={sp.pos} radius={4}
              pathOptions={{ color: '#0a0a12', fillColor: sp.color, fillOpacity: 1, weight: 2 }}>
              <LTooltip direction="top" offset={[0, -6]}>
                <span style={{ fontSize: 12 }}>{sp.name}</span>
              </LTooltip>
            </CircleMarker>
          ))}

          {mapData.labeledStops
            .filter(ls => ls.type === 'transfer')
            .map((ls, i) => (
              <CircleMarker key={`tr-${i}`} center={ls.pos} radius={8}
                pathOptions={{ color: '#0a0a12', fillColor: '#00e5ff', fillOpacity: 1, weight: 3 }}>
                <LTooltip direction="top" offset={[0, -10]} permanent className="stop-label">
                  <span>{ls.name}</span>
                </LTooltip>
              </CircleMarker>
            ))}

          {fromPos && (
            <Marker position={fromPos} icon={originIcon}>
              <LTooltip direction="top" offset={[0, -40]} permanent className="origin-label">
                <span>{from.name}</span>
              </LTooltip>
            </Marker>
          )}

          {toPos && (
            <Marker position={toPos} icon={destinationIcon}>
              <LTooltip direction="top" offset={[0, -40]} permanent className="dest-label">
                <span>{to.name}</span>
              </LTooltip>
            </Marker>
          )}
        </MapContainer>
      </Box>
    </Box>
    </LocalizationProvider>
  )
}
