import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  Alert, Autocomplete, Box, Button, Chip, CircularProgress, Divider,
  FormControlLabel, IconButton, MenuItem, Paper, Switch, TextField,
  ToggleButton, ToggleButtonGroup, Tooltip, Typography,
} from '@mui/material'
import {
  Add, Delete, Edit, Key, Place as PlaceIcon, Route as RouteIcon,
  Timeline, WarningAmber,
} from '@mui/icons-material'
import { useI18n } from '../i18n'
import {
  disruptionInstantToApi, disruptionInstantToInput, disruptionStartDefault,
} from '../utils'

// The back office authenticates writes with the same X-Api-Key as the GTFS
// reload endpoint. Kept in localStorage so an operator types it once per
// browser rather than once per edit.
const API_KEY_STORAGE = 'glove.disruptions.apiKey'

const CAUSES = ['works', 'incident', 'strike', 'event', 'weather', 'other']
const SCOPES = ['stop', 'line', 'line_section']

/** Read the stored API key, tolerating a cleared or corrupted entry. */
function readStoredKey() {
  try {
    return window.localStorage.getItem(API_KEY_STORAGE) || ''
  } catch (err) {
    console.warn('Cannot read the stored API key:', err)
    return ''
  }
}

/** Human-readable period, e.g. "01/09 22:00 → en cours". */
function formatPeriod(disruption, t, lang) {
  const opts = { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' }
  const start = new Date(disruption.starts_at).toLocaleString(lang, opts)
  if (!disruption.ends_at) return `${start} → ${t('disruptionOngoing')}`
  return `${start} → ${new Date(disruption.ends_at).toLocaleString(lang, opts)}`
}

/** Debounced remote search feeding an Autocomplete. */
function useRemoteOptions(url, mapItem) {
  const [options, setOptions] = useState([])
  const [loading, setLoading] = useState(false)

  const search = useCallback((query) => {
    if (!query || query.length < 2) { setOptions([]); return }
    setLoading(true)
    fetch(`${url}${encodeURIComponent(query)}&limit=10`)
      .then(res => res.json())
      .then(data => setOptions(mapItem(data)))
      .catch(err => {
        console.warn('Lookup failed:', err)
        setOptions([])
      })
      .finally(() => setLoading(false))
  }, [url, mapItem])

  return { options, loading, search }
}

/** Autocomplete over GTFS stops. */
function StopPicker({ label, value, onChange, required }) {
  const { t } = useI18n()
  // BAN addresses share the endpoint but cannot be disrupted: only GTFS stops
  // resolve to a stop index.
  const mapStops = useCallback(
    (data) => (data.places || []).filter(place => place.type === 'stop'),
    [],
  )
  const { options, loading, search } = useRemoteOptions('/api/places?q=', mapStops)

  return (
    <Autocomplete
      size="small"
      options={options}
      loading={loading}
      value={value}
      isOptionEqualToValue={(a, b) => a.id === b.id}
      getOptionLabel={(option) => option?.name || ''}
      onChange={(_, next) => onChange(next)}
      noOptionsText={t('typeToSearch')}
      renderInput={(params) => (
        <TextField
          {...params}
          label={label}
          required={required}
          onChange={(e) => search(e.target.value)}
          InputProps={{
            ...params.InputProps,
            endAdornment: (
              <>
                {loading ? <CircularProgress size={16} /> : null}
                {params.InputProps.endAdornment}
              </>
            ),
          }}
        />
      )}
    />
  )
}

/** Autocomplete over the dataset's lines. */
function LinePicker({ label, value, onChange, required }) {
  const { t } = useI18n()
  const mapLines = useCallback((data) => data.lines || [], [])
  const { options, loading, search } = useRemoteOptions('/api/lines?q=', mapLines)

  return (
    <Autocomplete
      size="small"
      options={options}
      loading={loading}
      value={value}
      isOptionEqualToValue={(a, b) => a.id === b.id}
      getOptionLabel={(option) => (option ? `${option.short_name} — ${option.long_name}` : '')}
      onChange={(_, next) => onChange(next)}
      noOptionsText={t('typeToSearch')}
      renderInput={(params) => (
        <TextField
          {...params}
          label={label}
          required={required}
          onChange={(e) => search(e.target.value)}
          InputProps={{
            ...params.InputProps,
            endAdornment: (
              <>
                {loading ? <CircularProgress size={16} /> : null}
                {params.InputProps.endAdornment}
              </>
            ),
          }}
        />
      )}
    />
  )
}

const EMPTY_FORM = {
  id: null,
  title: '',
  message: '',
  cause: 'works',
  severity: 'blocking',
  scopeType: 'stop',
  stop: null,
  line: null,
  fromStop: null,
  toStop: null,
  // Seeded with the current local time by startCreate: a disruption is
  // normally entered while it is already happening.
  startsAt: '',
  endsAt: '',
  ongoing: true,
}

export default function DisruptionsPanel() {
  const { t, lang } = useI18n()
  const [disruptions, setDisruptions] = useState([])
  const [loading, setLoading] = useState(true)
  const [feedback, setFeedback] = useState(null)
  const [onlyActive, setOnlyActive] = useState(false)
  const [apiKey, setApiKey] = useState(readStoredKey)
  const [showKey, setShowKey] = useState(false)
  const [form, setForm] = useState(null)
  const [reloadToken, setReloadToken] = useState(0)

  // Bumping a token rather than fetching here keeps the request inside the
  // effect below, where it can be cancelled.
  const refresh = useCallback(() => setReloadToken(token => token + 1), [])

  useEffect(() => {
    let cancelled = false
    const url = onlyActive ? '/api/disruptions?active_at=now' : '/api/disruptions'
    fetch(url)
      .then(res => res.json())
      .then(data => {
        if (cancelled) return
        setDisruptions(data.disruptions || [])
        setLoading(false)
      })
      .catch(err => {
        if (cancelled) return
        console.warn('Cannot load disruptions:', err)
        setFeedback({ severity: 'error', text: t('disruptionLoadFailed') })
        setLoading(false)
      })
    // Toggling the filter twice quickly must not let the first answer win.
    return () => { cancelled = true }
  }, [onlyActive, reloadToken, t])

  const saveKey = useCallback((value) => {
    setApiKey(value)
    try {
      window.localStorage.setItem(API_KEY_STORAGE, value)
    } catch (err) {
      console.warn('Cannot store the API key:', err)
    }
  }, [])

  const startCreate = useCallback(() => {
    setFeedback(null)
    setForm({ ...EMPTY_FORM, startsAt: disruptionStartDefault() })
  }, [])

  const startEdit = useCallback((disruption) => {
    setFeedback(null)
    const scope = disruption.scope
    setForm({
      id: disruption.id,
      title: disruption.title,
      message: disruption.message || '',
      cause: disruption.cause,
      severity: disruption.severity,
      scopeType: scope.type,
      // Only the identifier is stored, so the picker shows it until the
      // operator selects a fresh one.
      stop: scope.type === 'stop' ? { id: scope.stop_id, name: scope.stop_id } : null,
      line: scope.route_id ? { id: scope.route_id, short_name: scope.route_id, long_name: '' } : null,
      fromStop: scope.from_stop_id ? { id: scope.from_stop_id, name: scope.from_stop_id } : null,
      toStop: scope.to_stop_id ? { id: scope.to_stop_id, name: scope.to_stop_id } : null,
      startsAt: disruptionInstantToInput(disruption.starts_at),
      endsAt: disruptionInstantToInput(disruption.ends_at),
      ongoing: !disruption.ends_at,
    })
  }, [])

  const buildScope = useCallback((current) => {
    if (current.scopeType === 'stop') {
      return current.stop ? { type: 'stop', stop_id: current.stop.id } : null
    }
    if (current.scopeType === 'line') {
      return current.line ? { type: 'line', route_id: current.line.id } : null
    }
    if (!current.line || !current.fromStop || !current.toStop) return null
    return {
      type: 'line_section',
      route_id: current.line.id,
      from_stop_id: current.fromStop.id,
      to_stop_id: current.toStop.id,
    }
  }, [])

  const submit = useCallback(async (event) => {
    event.preventDefault()
    const scope = buildScope(form)
    if (!scope) {
      setFeedback({ severity: 'error', text: t('disruptionScopeIncomplete') })
      return
    }

    const body = {
      title: form.title,
      message: form.message,
      cause: form.cause,
      severity: form.severity,
      scope,
      starts_at: disruptionInstantToApi(form.startsAt),
      ends_at: form.ongoing ? null : disruptionInstantToApi(form.endsAt),
    }

    try {
      const res = await fetch(
        form.id ? `/api/disruptions/${form.id}` : '/api/disruptions',
        {
          method: form.id ? 'PUT' : 'POST',
          headers: { 'Content-Type': 'application/json', 'X-Api-Key': apiKey },
          body: JSON.stringify(body),
        },
      )
      if (!res.ok) {
        const payload = await res.json().catch(() => null)
        setFeedback({
          severity: 'error',
          text: payload?.error?.message || `HTTP ${res.status}`,
        })
        return
      }
      setFeedback({ severity: 'success', text: t('disruptionSaved') })
      setForm(null)
      refresh()
    } catch (err) {
      console.warn('Cannot save the disruption:', err)
      setFeedback({ severity: 'error', text: err.message })
    }
  }, [form, apiKey, buildScope, refresh, t])

  const remove = useCallback(async (id) => {
    try {
      const res = await fetch(`/api/disruptions/${id}`, {
        method: 'DELETE',
        headers: { 'X-Api-Key': apiKey },
      })
      if (!res.ok) {
        const payload = await res.json().catch(() => null)
        setFeedback({
          severity: 'error',
          text: payload?.error?.message || `HTTP ${res.status}`,
        })
        return
      }
      setFeedback({ severity: 'success', text: t('disruptionDeleted') })
      refresh()
    } catch (err) {
      console.warn('Cannot delete the disruption:', err)
      setFeedback({ severity: 'error', text: err.message })
    }
  }, [apiKey, refresh, t])

  const scopeSummary = useCallback((scope) => {
    if (scope.type === 'stop') return scope.stop_id
    if (scope.type === 'line') return scope.route_id
    return `${scope.route_id}: ${scope.from_stop_id} → ${scope.to_stop_id}`
  }, [])

  const scopeIcon = useMemo(() => ({
    stop: <PlaceIcon sx={{ fontSize: 14 }} />,
    line: <RouteIcon sx={{ fontSize: 14 }} />,
    line_section: <Timeline sx={{ fontSize: 14 }} />,
  }), [])

  return (
    <Box sx={{ overflow: 'auto', flex: 1, pb: 3 }}>
      <Box sx={{ px: 2.5, pt: 2, pb: 1, display: 'flex', alignItems: 'center', gap: 1 }}>
        <Typography variant="overline" color="primary.main" fontWeight={700} letterSpacing={2} fontSize={10}>
          {t('disruptionsTitle')}
        </Typography>
        <Box sx={{ flex: 1 }} />
        <Tooltip title={t('disruptionApiKey')}>
          <IconButton size="small" aria-label={t('disruptionApiKey')} onClick={() => setShowKey(v => !v)}>
            <Key sx={{ fontSize: 16, color: apiKey ? '#00e5ff' : '#6b6980' }} />
          </IconButton>
        </Tooltip>
        <Tooltip title={t('disruptionNew')}>
          <IconButton size="small" aria-label={t('disruptionNew')} onClick={startCreate}>
            <Add sx={{ fontSize: 18, color: '#00e5ff' }} />
          </IconButton>
        </Tooltip>
      </Box>

      {showKey && (
        <Box sx={{ px: 2.5, pb: 1.5 }}>
          <TextField
            fullWidth size="small" type="password" label={t('disruptionApiKey')}
            value={apiKey} onChange={(e) => saveKey(e.target.value)}
            helperText={t('disruptionApiKeyHelp')}
          />
        </Box>
      )}

      {feedback && (
        <Box sx={{ px: 2.5, pb: 1.5 }}>
          <Alert severity={feedback.severity} onClose={() => setFeedback(null)}>
            {feedback.text}
          </Alert>
        </Box>
      )}

      {form && (
        <Box component="form" onSubmit={submit} sx={{ px: 2.5, pb: 2 }}>
          <Paper sx={{ p: 2, bgcolor: 'rgba(20, 20, 35, 0.5)', display: 'flex', flexDirection: 'column', gap: 1.5 }}>
            <Typography variant="subtitle2" fontWeight={700}>
              {form.id ? t('disruptionEdit') : t('disruptionNew')}
            </Typography>

            <TextField
              size="small" required label={t('disruptionTitleField')} value={form.title}
              onChange={(e) => setForm({ ...form, title: e.target.value })}
            />
            <TextField
              size="small" multiline minRows={2} label={t('disruptionMessage')} value={form.message}
              onChange={(e) => setForm({ ...form, message: e.target.value })}
            />

            <Box sx={{ display: 'flex', gap: 1.5 }}>
              <TextField
                select size="small" fullWidth label={t('disruptionCause')} value={form.cause}
                onChange={(e) => setForm({ ...form, cause: e.target.value })}
              >
                {CAUSES.map(cause => (
                  <MenuItem key={cause} value={cause}>{t(`cause_${cause}`)}</MenuItem>
                ))}
              </TextField>
              <TextField
                select size="small" fullWidth label={t('disruptionSeverity')} value={form.severity}
                onChange={(e) => setForm({ ...form, severity: e.target.value })}
              >
                <MenuItem value="blocking">{t('severity_blocking')}</MenuItem>
                <MenuItem value="info">{t('severity_info')}</MenuItem>
              </TextField>
            </Box>

            <ToggleButtonGroup
              size="small" exclusive fullWidth value={form.scopeType}
              onChange={(_, next) => next && setForm({ ...form, scopeType: next })}
            >
              {SCOPES.map(scope => (
                <ToggleButton key={scope} value={scope} aria-label={t(`scope_${scope}`)}>
                  {t(`scope_${scope}`)}
                </ToggleButton>
              ))}
            </ToggleButtonGroup>

            {form.scopeType === 'stop' && (
              <StopPicker
                required label={t('scope_stop')} value={form.stop}
                onChange={(stop) => setForm({ ...form, stop })}
              />
            )}
            {form.scopeType === 'line' && (
              <LinePicker
                required label={t('scope_line')} value={form.line}
                onChange={(line) => setForm({ ...form, line })}
              />
            )}
            {form.scopeType === 'line_section' && (
              <>
                <LinePicker
                  required label={t('scope_line')} value={form.line}
                  onChange={(line) => setForm({ ...form, line })}
                />
                <StopPicker
                  required label={t('disruptionFromStop')} value={form.fromStop}
                  onChange={(fromStop) => setForm({ ...form, fromStop })}
                />
                <StopPicker
                  required label={t('disruptionToStop')} value={form.toStop}
                  onChange={(toStop) => setForm({ ...form, toStop })}
                />
              </>
            )}

            <TextField
              size="small" required type="datetime-local" label={t('disruptionStartsAt')}
              InputLabelProps={{ shrink: true }} value={form.startsAt}
              onChange={(e) => setForm({ ...form, startsAt: e.target.value })}
            />
            <FormControlLabel
              control={
                <Switch
                  size="small" checked={form.ongoing}
                  onChange={(e) => setForm({ ...form, ongoing: e.target.checked })}
                />
              }
              label={<Typography variant="caption">{t('disruptionNoEnd')}</Typography>}
            />
            {!form.ongoing && (
              <TextField
                size="small" required type="datetime-local" label={t('disruptionEndsAt')}
                InputLabelProps={{ shrink: true }} value={form.endsAt}
                onChange={(e) => setForm({ ...form, endsAt: e.target.value })}
              />
            )}

            <Box sx={{ display: 'flex', gap: 1, justifyContent: 'flex-end' }}>
              <Button size="small" onClick={() => setForm(null)}>{t('cancel')}</Button>
              <Button size="small" variant="contained" type="submit">{t('save')}</Button>
            </Box>
          </Paper>
        </Box>
      )}

      <Box sx={{ px: 2.5, pb: 1 }}>
        <FormControlLabel
          control={
            <Switch size="small" checked={onlyActive} onChange={(e) => setOnlyActive(e.target.checked)} />
          }
          label={<Typography variant="caption">{t('disruptionOnlyActive')}</Typography>}
        />
      </Box>
      <Divider sx={{ borderColor: 'rgba(255,255,255,0.06)' }} />

      {loading ? (
        <Box sx={{ p: 4, textAlign: 'center' }}>
          <CircularProgress size={24} sx={{ color: '#00e5ff' }} />
        </Box>
      ) : disruptions.length === 0 ? (
        <Box sx={{ p: 4, textAlign: 'center' }}>
          <Typography variant="body2" color="text.secondary">{t('disruptionNone')}</Typography>
        </Box>
      ) : (
        disruptions.map(disruption => (
          <Box
            key={disruption.id}
            sx={{
              px: 2.5, py: 1.5,
              borderBottom: '1px solid rgba(255,255,255,0.04)',
              '&:hover': { bgcolor: 'rgba(255,255,255,0.02)' },
            }}
          >
            <Box sx={{ display: 'flex', alignItems: 'flex-start', gap: 1 }}>
              <WarningAmber
                sx={{
                  fontSize: 16, mt: 0.3,
                  color: disruption.severity === 'blocking' ? '#ff5252' : '#ffb800',
                }}
              />
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Typography variant="body2" fontWeight={600} noWrap>{disruption.title}</Typography>
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.75, mt: 0.5, flexWrap: 'wrap' }}>
                  <Chip
                    size="small" icon={scopeIcon[disruption.scope.type]}
                    label={scopeSummary(disruption.scope)}
                    sx={{ height: 20, fontSize: 10 }}
                  />
                  <Chip
                    size="small" label={t(`cause_${disruption.cause}`)}
                    sx={{ height: 20, fontSize: 10 }}
                  />
                </Box>
                <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 0.5 }}>
                  {formatPeriod(disruption, t, lang)}
                </Typography>
                {disruption.message && (
                  <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 0.25 }}>
                    {disruption.message}
                  </Typography>
                )}
              </Box>
              <IconButton size="small" aria-label={t('disruptionEdit')} onClick={() => startEdit(disruption)}>
                <Edit sx={{ fontSize: 15 }} />
              </IconButton>
              <IconButton size="small" aria-label={t('disruptionDelete')} onClick={() => remove(disruption.id)}>
                <Delete sx={{ fontSize: 15 }} />
              </IconButton>
            </Box>
          </Box>
        ))
      )}
    </Box>
  )
}
