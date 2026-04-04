import { useState, useEffect, useRef, useCallback } from 'react'
import {
  Typography, Paper, TextField, Button,
  Box, Card, CardContent, CardActionArea, Chip, Collapse, Alert,
  CircularProgress, Divider, Stack, IconButton, Tooltip, Autocomplete, alpha,
} from '@mui/material'
import {
  Search, SwapVert, DirectionsBus, Train, Tram, Subway,
  ExpandMore, ExpandLess, TransferWithinAStation,
  NearMe, ArrowRightAlt, Place, AccessTime, Settings,
  Route, Timer, MultipleStop, Storage, CalendarMonth, Close, Language,
} from '@mui/icons-material'
import { MapContainer, TileLayer, Polyline, CircleMarker, Marker, Tooltip as LTooltip, useMap } from 'react-leaflet'
import L from 'leaflet'
import 'leaflet/dist/leaflet.css'
import { useI18n } from './i18n.jsx'

function flagIcon(color) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="28" height="40" viewBox="0 0 28 40">
    <line x1="6" y1="4" x2="6" y2="38" stroke="${color}" stroke-width="2.5" stroke-linecap="round"/>
    <path d="M6 4 L24 10 L6 18 Z" fill="${color}" opacity="0.9"/>
    <circle cx="6" cy="38" r="3" fill="${color}"/>
  </svg>`
  return L.divIcon({ html: svg, iconSize: [28, 40], iconAnchor: [6, 38], className: '' })
}

const originIcon = flagIcon('#22c55e')
const destinationIcon = flagIcon('#ef4444')
const IDF_CENTER = [48.8566, 2.3522]
const IDF_ZOOM = 11

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

function modeIcon(mode) {
  switch (mode) {
    case 'metro': return <Subway fontSize="small" />
    case 'rail': return <Train fontSize="small" />
    case 'tramway': return <Tram fontSize="small" />
    default: return <DirectionsBus fontSize="small" />
  }
}

function modeColor(mode, color) {
  if (color) return `#${color}`
  switch (mode) {
    case 'metro': return '#003ca6'
    case 'rail': return '#333'
    case 'tramway': return '#6b9c2a'
    case 'bus': return '#95c11f'
    default: return '#757575'
  }
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
    if (!newInput || newInput.length < 2) { setOptions(value ? [value] : []); setLoading(false); return }
    setLoading(true)
    fetchPlaces(newInput, (results) => { setOptions(results); setLoading(false) })
  }, [fetchPlaces, value])

  const displayOptions = (!inputValue || inputValue.length < 2) ? (value ? [value] : []) : options

  return (
    <Autocomplete
      fullWidth size="small" options={displayOptions}
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
      renderOption={(props, option) => {
        const { key, ...rest } = props
        return (
          <Box component="li" key={key} {...rest}
            sx={{ display: 'flex', alignItems: 'center', gap: 1.5, py: 1 }}>
            <Place fontSize="small" sx={{ color: 'primary.main' }} />
            <Box>
              <Typography variant="body2" fontWeight={500}>{option.name}</Typography>
              <Typography variant="caption" color="text.secondary">{option.id}</Typography>
            </Box>
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
  fastest: '#16a34a',
  least_transfers: '#2563eb',
  least_walking: '#9333ea',
}

function JourneyCard({ journey, rank, selected, onSelect }) {
  const { t } = useI18n()
  const [open, setOpen] = useState(false)
  const ptSections = journey.sections.filter(s => s.type === 'public_transport' && s.display_informations)

  return (
    <Card
      sx={{
        mb: 1.5, border: '2px solid',
        borderColor: selected ? 'primary.main' : 'transparent',
        bgcolor: selected ? (th) => alpha(th.palette.primary.main, 0.04) : 'background.paper',
        '&:hover': { boxShadow: 4, transform: 'translateY(-1px)' },
      }}
      elevation={selected ? 2 : 0}
    >
      <CardActionArea onClick={() => { setOpen(!open); onSelect() }}>
        <CardContent sx={{ py: 1.5, px: 2.5 }}>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
            <Box sx={{
              width: 36, height: 36, borderRadius: '50%',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              bgcolor: rank === 1 ? 'primary.main' : (th) => alpha(th.palette.text.secondary, 0.1),
              color: rank === 1 ? 'white' : 'text.secondary',
              fontWeight: 700, fontSize: 14, flexShrink: 0,
            }}>
              {rank}
            </Box>

            <Box sx={{ flex: 1, minWidth: 0 }}>
              <Typography variant="body2" fontWeight={600}>
                {formatTime(journey.departure_date_time)}
                <ArrowRightAlt sx={{ verticalAlign: 'middle', mx: 0.5, opacity: 0.5 }} fontSize="small" />
                {formatTime(journey.arrival_date_time)}
              </Typography>
              <Stack direction="row" spacing={0.5} sx={{ mt: 0.5 }} flexWrap="wrap" useFlexGap>
                {ptSections.map((s, i) => {
                  const di = s.display_informations
                  const bg = modeColor(di.commercial_mode, di.color)
                  const fg = di.text_color ? `#${di.text_color}` : '#fff'
                  return (
                    <Box key={i} sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.3 }}>
                      {i > 0 && <Typography variant="caption" sx={{ color: 'text.disabled', mx: 0.2 }}>›</Typography>}
                      <Chip icon={modeIcon(di.commercial_mode)} label={di.label || di.commercial_mode}
                        size="small"
                        sx={{ bgcolor: bg, color: fg, fontWeight: 700, fontSize: 11, height: 24,
                          '& .MuiChip-icon': { color: fg, ml: 0.5 } }} />
                    </Box>
                  )
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
                        sx={{ height: 20, fontSize: 10, fontWeight: 600, color, borderColor: color }} />
                    )
                  })}
                </Stack>
              )}
            </Box>

            <Box sx={{ textAlign: 'right', flexShrink: 0, pl: 1 }}>
              <Typography variant="body1" fontWeight={800} lineHeight={1.2} color="text.primary">
                {formatDuration(journey.duration)}
              </Typography>
              {journey.nb_transfers > 0 && (
                <Typography variant="caption" sx={{ color: '#f59e0b' }} fontWeight={600}>
                  {journey.nb_transfers} {t('transfers')}
                </Typography>
              )}
            </Box>
            {open ? <ExpandLess fontSize="small" color="action" /> : <ExpandMore fontSize="small" color="action" />}
          </Box>
        </CardContent>
      </CardActionArea>

      <Collapse in={open}>
        <Divider />
        <Box sx={{ px: 2.5, py: 1.5 }}>
          {journey.sections.map((s, i) => {
            const isPt = s.type === 'public_transport'
            const di = s.display_informations
            const lineColor = isPt ? modeColor(di?.commercial_mode, di?.color) : '#e2e8f0'
            return (
              <Box key={i} sx={{
                display: 'flex', gap: 1.5, py: 1,
                borderBottom: i < journey.sections.length - 1 ? '1px solid' : 'none',
                borderColor: 'divider',
              }}>
                <Typography variant="caption" fontWeight={600} sx={{ width: 40, flexShrink: 0, pt: 0.2, color: 'text.primary' }}>
                  {formatTime(s.departure_date_time)}
                </Typography>
                <Box sx={{ width: 4, borderRadius: 2, bgcolor: lineColor, flexShrink: 0 }} />
                <Box sx={{ flex: 1, minWidth: 0 }}>
                  <Typography variant="caption" fontWeight={600} color="text.primary">
                    {isPt ? <>{di?.commercial_mode} <strong>{di?.label}</strong></> : (
                      <Box component="span" sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}>
                        <TransferWithinAStation sx={{ fontSize: 14 }} color="action" /> {t('transfer')}
                      </Box>
                    )}
                  </Typography>
                  <Typography variant="caption" color="text.secondary" display="block" noWrap>
                    {s.from.name} → {s.to.name}
                  </Typography>
                  {isPt && di?.direction && (
                    <Typography variant="caption" color="text.disabled" display="block" fontStyle="italic" noWrap>
                      {t('direction')} {di.direction}
                    </Typography>
                  )}
                </Box>
                <Typography variant="caption" color="text.secondary" sx={{ flexShrink: 0, pt: 0.2 }}>
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
      <CircularProgress size={28} />
      <Typography variant="body2" color="text.secondary" sx={{ mt: 1.5 }}>{t('loadingStatus')}</Typography>
    </Box>
  )

  return (
    <Box sx={{ overflow: 'auto', flex: 1 }}>
      <Box sx={{ px: 2.5, pt: 2, pb: 0.5 }}>
        <Typography variant="overline" color="text.secondary" fontWeight={700} letterSpacing={1}>
          {t('gtfsData')}
        </Typography>
      </Box>
      {items.map((item, i) => (
        <Box key={i} sx={{ display: 'flex', alignItems: 'center', gap: 1.5, px: 2.5, py: 0.8 }}>
          <Box sx={{ color: 'primary.main', display: 'flex' }}>{item.icon}</Box>
          <Typography variant="body2" sx={{ flex: 1 }}>{item.label}</Typography>
          <Typography variant="body2" fontWeight={700}>{item.value?.toLocaleString() ?? '—'}</Typography>
        </Box>
      ))}

      <Divider sx={{ my: 1.5, mx: 2 }} />

      <Box sx={{ px: 2.5, pb: 0.5 }}>
        <Typography variant="overline" color="text.secondary" fontWeight={700} letterSpacing={1}>
          {t('raptorIndex')}
        </Typography>
      </Box>
      {raptorItems.map((item, i) => (
        <Box key={i} sx={{ display: 'flex', alignItems: 'center', gap: 1.5, px: 2.5, py: 0.8 }}>
          <Box sx={{ color: 'secondary.main', display: 'flex' }}>{item.icon}</Box>
          <Typography variant="body2" sx={{ flex: 1 }}>{item.label}</Typography>
          <Typography variant="body2" fontWeight={700}>{item.value?.toLocaleString() ?? '—'}</Typography>
        </Box>
      ))}

      <Divider sx={{ my: 1.5, mx: 2 }} />

      <Box sx={{ px: 2.5, pb: 1 }}>
        <Typography variant="overline" color="text.secondary" fontWeight={700} letterSpacing={1}>
          {t('lastLoaded')}
        </Typography>
        <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
          {status.loaded_at
            ? new Date(status.loaded_at).toLocaleString(undefined, { dateStyle: 'full', timeStyle: 'medium' })
            : '—'}
        </Typography>
        <Chip label={status.status || '—'} size="small" color="success" variant="outlined"
          sx={{ mt: 1, fontWeight: 600, textTransform: 'uppercase', fontSize: 11 }} />
      </Box>

      <Divider sx={{ my: 1.5, mx: 2 }} />

      <Box sx={{ px: 2.5, pb: 2 }}>
        {reloadMsg && (
          <Alert severity={reloadMsg.severity} sx={{ mb: 1.5, borderRadius: 2 }}>{reloadMsg.text}</Alert>
        )}
        <Button variant="contained" fullWidth onClick={handleReload} disabled={reloading}
          startIcon={reloading ? <CircularProgress size={18} color="inherit" /> : <Storage />}
          sx={{
            py: 1.2,
            background: 'linear-gradient(135deg, #0ea5e9 0%, #6366f1 100%)',
            '&:hover': { background: 'linear-gradient(135deg, #0284c7 0%, #4f46e5 100%)' },
          }}>
          {reloading ? t('reloading') : t('reloadGtfs')}
        </Button>
      </Box>

      <Divider sx={{ my: 1, mx: 2 }} />

      <Box sx={{ px: 2.5, py: 1.5 }}>
        <Typography variant="overline" color="text.secondary" fontWeight={700} letterSpacing={1}>
          {t('about')}
        </Typography>
        <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>{t('aboutDesc')}</Typography>
        <Typography variant="caption" color="text.disabled">{t('aboutTech')}</Typography>
      </Box>
    </Box>
  )
}

// --- Main App ---

export default function App() {
  const { t, lang, setLang } = useI18n()

  const [from, setFrom] = useState(null)
  const [to, setTo] = useState(null)
  const [datetime, setDatetime] = useState(defaultDatetime())
  const [journeys, setJourneys] = useState(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(null)
  const [status, setStatus] = useState(null)
  const [selectedJourney, setSelectedJourney] = useState(0)
  const [view, setView] = useState('search')

  const refreshStatus = () => {
    fetch('/api/status').then(r => r.json()).then(setStatus).catch(() => {})
  }
  useEffect(() => { refreshStatus() }, [])

  const clearResults = () => { setJourneys(null); setSelectedJourney(0); setError(null) }
  const handleFromChange = (v) => { setFrom(v); clearResults() }
  const handleToChange = (v) => { setTo(v); clearResults() }
  const swap = () => { setFrom(to); setTo(from); clearResults() }

  const search = async (e) => {
    e.preventDefault()
    if (!from || !to) return
    setLoading(true); setError(null); setJourneys(null); setSelectedJourney(0)
    try {
      const params = new URLSearchParams({ from: from.id, to: to.id, datetime: toApiDatetime(datetime) })
      const res = await fetch(`/api/journeys?${params}`)
      const data = await res.json()
      if (data.error) setError(data.error.message)
      else setJourneys(data.journeys)
    } catch (err) {
      setError(err.message)
    } finally { setLoading(false) }
  }

  const selectedJ = journeys?.[selectedJourney]
  const mapData = selectedJ ? extractMapData(selectedJ) : { lines: [], stopPoints: [], labeledStops: [] }
  const allCoords = mapData.lines.flatMap(l => l.coords)
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
    <Box sx={{ display: 'flex', height: '100vh', overflow: 'hidden' }}>
      {/* Left panel */}
      <Box sx={{
        width: 480, minWidth: 480, bgcolor: 'background.default',
        display: 'flex', flexDirection: 'column',
        borderRight: '1px solid', borderColor: 'divider',
        zIndex: 1200, overflow: 'hidden',
      }}>
        {/* Header */}
        <Box sx={{
          px: 3, py: 2,
          background: 'linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%)',
          color: 'white',
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          flexShrink: 0,
        }}>
          <Stack direction="row" alignItems="center" spacing={1} sx={{ cursor: 'pointer' }}
            onClick={() => setView('search')}>
            <NearMe fontSize="small" />
            <Typography variant="h6">Glove</Typography>
          </Stack>
          <Stack direction="row" spacing={0.5}>
            <Tooltip title={lang === 'fr' ? 'English' : 'Français'}>
              <IconButton onClick={toggleLang} size="small"
                sx={{ color: 'rgba(255,255,255,0.8)', '&:hover': { color: 'white', bgcolor: 'rgba(255,255,255,0.15)' } }}>
                <Language fontSize="small" />
              </IconButton>
            </Tooltip>
            <Tooltip title={view === 'settings' ? t('search') : t('settings')}>
              <IconButton
                onClick={() => setView(view === 'settings' ? 'search' : 'settings')}
                size="small"
                sx={{ color: 'rgba(255,255,255,0.8)', '&:hover': { color: 'white', bgcolor: 'rgba(255,255,255,0.15)' } }}>
                {view === 'settings' ? <Close fontSize="small" /> : <Settings fontSize="small" />}
              </IconButton>
            </Tooltip>
          </Stack>
        </Box>

        {view === 'settings' ? (
          <SettingsPanel status={status} onReload={refreshStatus} />
        ) : (
          <>
            {/* Search form */}
            <Box sx={{ p: 2.5, flexShrink: 0 }}>
              <Paper component="form" onSubmit={search} sx={{ p: 2.5 }} elevation={0} variant="outlined">
                <Stack spacing={2}>
                  <Box sx={{ display: 'flex', gap: 1, alignItems: 'center' }}>
                    <Box sx={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 1.5 }}>
                      <PlaceAutocomplete label={t('departure')} value={from} onChange={handleFromChange}
                        icon={<Box sx={{ width: 8, height: 8, borderRadius: '50%', bgcolor: '#22c55e', ml: 1 }} />} />
                      <PlaceAutocomplete label={t('arrival')} value={to} onChange={handleToChange}
                        icon={<Box sx={{ width: 8, height: 8, borderRadius: '50%', bgcolor: '#ef4444', ml: 1 }} />} />
                    </Box>
                    <Tooltip title={t('swap')}>
                      <IconButton onClick={swap} size="small"
                        sx={{ border: '1px solid', borderColor: 'divider', borderRadius: 2, p: 0.8 }}>
                        <SwapVert fontSize="small" />
                      </IconButton>
                    </Tooltip>
                  </Box>

                  <TextField label={t('dateTime')} type="datetime-local" value={datetime}
                    onChange={e => setDatetime(e.target.value)} size="small" fullWidth
                    slotProps={{
                      inputLabel: { shrink: true },
                      input: {
                        startAdornment: <><AccessTime fontSize="small" sx={{ color: 'text.disabled', mr: 0.5, ml: 0.5 }} /></>,
                      },
                    }}
                  />

                  <Button type="submit" variant="contained" fullWidth size="large"
                    disabled={loading || !from || !to}
                    startIcon={loading ? <CircularProgress size={18} color="inherit" /> : <Search />}
                    sx={{
                      py: 1.3,
                      background: 'linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%)',
                      '&:hover': { background: 'linear-gradient(135deg, #4f46e5 0%, #7c3aed 100%)' },
                    }}>
                    {loading ? t('searching') : t('search')}
                  </Button>
                </Stack>
              </Paper>
            </Box>

            {/* Results */}
            <Box sx={{ flex: 1, overflow: 'auto', px: 2.5, pb: 2.5 }}>
              {error && <Alert severity="error" sx={{ mb: 1.5, borderRadius: 2 }}>{error}</Alert>}

              {journeys && journeys.length === 0 && (
                <Alert severity="info" sx={{ borderRadius: 2 }}>{t('noResults')}</Alert>
              )}

              {journeys && journeys.length > 0 && (
                <>
                  <Typography variant="caption" color="text.secondary" sx={{ mb: 1, display: 'block' }}>
                    {resultsText}
                  </Typography>
                  {journeys.map((j, i) => (
                    <JourneyCard key={i} journey={j} rank={i + 1}
                      selected={i === selectedJourney}
                      onSelect={() => setSelectedJourney(i)} />
                  ))}
                </>
              )}
            </Box>
          </>
        )}
      </Box>

      {/* Map */}
      <Box sx={{ flex: 1, position: 'relative' }}>
        <MapContainer center={IDF_CENTER} zoom={IDF_ZOOM}
          style={{ height: '100%', width: '100%' }} zoomControl={false}>
          <TileLayer
            attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OSM</a> &copy; <a href="https://carto.com/">CARTO</a>'
            url="https://{s}.basemaps.cartocdn.com/rastertiles/voyager_nolabels/{z}/{x}/{y}{r}.png"
          />

          {fitBounds && <FitBounds bounds={fitBounds} />}
          {flyTo && <FlyToPoint point={flyTo} />}

          {fromPos && toPos && !hasResults && (
            <Polyline positions={[fromPos, toPos]}
              pathOptions={{ color: '#94a3b8', weight: 2, opacity: 0.7, dashArray: '8, 8' }} />
          )}

          {mapData.lines.map((line, i) => (
            <Polyline key={`line-${i}`} positions={line.coords}
              pathOptions={{ color: line.color, weight: 5, opacity: 0.85 }} />
          ))}

          {mapData.stopPoints.map((sp, i) => (
            <CircleMarker key={`sp-${i}`} center={sp.pos} radius={4}
              pathOptions={{ color: '#fff', fillColor: sp.color, fillOpacity: 1, weight: 2 }}>
              <LTooltip direction="top" offset={[0, -6]}>
                <span style={{ fontSize: 12 }}>{sp.name}</span>
              </LTooltip>
            </CircleMarker>
          ))}

          {mapData.labeledStops
            .filter(ls => ls.type === 'transfer')
            .map((ls, i) => (
              <CircleMarker key={`tr-${i}`} center={ls.pos} radius={8}
                pathOptions={{ color: '#fff', fillColor: '#6366f1', fillOpacity: 1, weight: 3 }}>
                <LTooltip direction="top" offset={[0, -10]} permanent className="stop-label">
                  <span>{ls.name}</span>
                </LTooltip>
              </CircleMarker>
            ))}

          {fromPos && (
            <Marker position={fromPos} icon={originIcon}>
              <LTooltip direction="top" offset={[0, -40]} permanent className="stop-label">
                <span>{from.name}</span>
              </LTooltip>
            </Marker>
          )}

          {toPos && (
            <Marker position={toPos} icon={destinationIcon}>
              <LTooltip direction="top" offset={[0, -40]} permanent className="stop-label">
                <span>{to.name}</span>
              </LTooltip>
            </Marker>
          )}
        </MapContainer>
      </Box>
    </Box>
  )
}
