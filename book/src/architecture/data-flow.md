# Data Flow

## Startup Sequence

<svg viewBox="0 0 560 520" xmlns="http://www.w3.org/2000/svg" style="max-width:560px;width:100%;font-family:'DM Sans',sans-serif;">
  <defs>
    <marker id="a1" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#8b89a0"/></marker>
    <linearGradient id="cg" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00e5ff" stop-opacity="0.12"/><stop offset="100%" stop-color="#00e5ff" stop-opacity="0.04"/></linearGradient>
    <linearGradient id="ag" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#ffb800" stop-opacity="0.15"/><stop offset="100%" stop-color="#ffb800" stop-opacity="0.05"/></linearGradient>
    <linearGradient id="gg" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00e676" stop-opacity="0.12"/><stop offset="100%" stop-color="#00e676" stop-opacity="0.04"/></linearGradient>
  </defs>
  <!-- config.yaml -->
  <rect x="200" y="10" width="160" height="40" rx="8" fill="rgba(255,255,255,0.04)" stroke="rgba(255,255,255,0.12)" stroke-width="1"/>
  <text x="280" y="35" text-anchor="middle" fill="#e4e2ec" font-size="12" font-weight="600">config.yaml</text>
  <line x1="280" y1="50" x2="280" y2="75" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- Load Config -->
  <rect x="180" y="80" width="200" height="44" rx="8" fill="url(#cg)" stroke="#00e5ff" stroke-opacity="0.4" stroke-width="1"/>
  <text x="280" y="107" text-anchor="middle" fill="#00e5ff" font-size="12" font-weight="600">Load Config → Check Cache</text>
  <line x1="280" y1="124" x2="280" y2="150" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- Diamond: Cache valid? -->
  <polygon points="280,155 340,185 280,215 220,185" fill="url(#ag)" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1.5"/>
  <text x="280" y="189" text-anchor="middle" fill="#ffb800" font-size="10" font-weight="600">Cache valid?</text>
  <!-- Yes branch (left) -->
  <line x1="220" y1="185" x2="120" y2="185" stroke="#8b89a0" stroke-width="1" marker-end="url(#a1)"/>
  <text x="170" y="178" text-anchor="middle" fill="#00e676" font-size="9" font-weight="600">YES</text>
  <rect x="20" y="165" width="100" height="44" rx="8" fill="url(#gg)" stroke="#00e676" stroke-opacity="0.4" stroke-width="1"/>
  <text x="70" y="184" text-anchor="middle" fill="#00e676" font-size="10" font-weight="600">Load cache</text>
  <text x="70" y="199" text-anchor="middle" fill="#56546a" font-size="9">sub-second</text>
  <!-- No branch (right) -->
  <line x1="340" y1="185" x2="440" y2="185" stroke="#8b89a0" stroke-width="1" marker-end="url(#a1)"/>
  <text x="390" y="178" text-anchor="middle" fill="#ff5252" font-size="9" font-weight="600">NO</text>
  <rect x="440" y="160" width="110" height="55" rx="8" fill="url(#ag)" stroke="#ffb800" stroke-opacity="0.4" stroke-width="1"/>
  <text x="495" y="182" text-anchor="middle" fill="#ffb800" font-size="10" font-weight="600">Parse GTFS</text>
  <text x="495" y="196" text-anchor="middle" fill="#8b89a0" font-size="9">Build RAPTOR</text>
  <text x="495" y="209" text-anchor="middle" fill="#56546a" font-size="8">10-30 seconds</text>
  <!-- Merge -->
  <line x1="70" y1="209" x2="70" y2="280" stroke="#8b89a0" stroke-width="1"/>
  <line x1="495" y1="215" x2="495" y2="280" stroke="#8b89a0" stroke-width="1"/>
  <line x1="70" y1="280" x2="495" y2="280" stroke="#8b89a0" stroke-width="1"/>
  <line x1="280" y1="280" x2="280" y2="310" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- BAN -->
  <rect x="180" y="315" width="200" height="44" rx="8" fill="url(#cg)" stroke="#00e5ff" stroke-opacity="0.3" stroke-width="1"/>
  <text x="280" y="335" text-anchor="middle" fill="#e4e2ec" font-size="12" font-weight="600">Load BAN data</text>
  <text x="280" y="351" text-anchor="middle" fill="#56546a" font-size="10">addresses</text>
  <line x1="280" y1="359" x2="280" y2="395" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- Actix -->
  <rect x="150" y="400" width="260" height="50" rx="10" fill="url(#gg)" stroke="#00e676" stroke-opacity="0.4" stroke-width="1.5"/>
  <text x="280" y="422" text-anchor="middle" fill="#00e676" font-size="13" font-weight="700">Start Actix-web</text>
  <text x="280" y="440" text-anchor="middle" fill="#8b89a0" font-size="10">Serve API + SPA</text>
</svg>

## GTFS Data Model

Glove loads the following GTFS files:

| File | Content | Rust Struct |
|------|---------|-------------|
| `agency.txt` | Transit agencies | `Agency` |
| `routes.txt` | Transit routes (lines) | `Route` |
| `stops.txt` | Stop locations | `Stop` |
| `trips.txt` | Individual trips | `Trip` |
| `stop_times.txt` | Arrival/departure at each stop | `StopTime` |
| `calendar.txt` | Weekly service schedules | `Calendar` |
| `calendar_dates.txt` | Service exceptions | `CalendarDate` |
| `transfers.txt` | Transfer connections between stops | `Transfer` |

## Query Flow

### Public Transit Journey

<svg viewBox="0 0 480 520" xmlns="http://www.w3.org/2000/svg" style="max-width:480px;width:100%;font-family:'DM Sans',sans-serif;">
  <defs>
    <marker id="a2" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#8b89a0"/></marker>
    <marker id="a3" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#ffb800"/></marker>
    <linearGradient id="c2" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00e5ff" stop-opacity="0.12"/><stop offset="100%" stop-color="#00e5ff" stop-opacity="0.04"/></linearGradient>
    <linearGradient id="a4" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#ffb800" stop-opacity="0.15"/><stop offset="100%" stop-color="#ffb800" stop-opacity="0.05"/></linearGradient>
    <linearGradient id="g2" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00e676" stop-opacity="0.12"/><stop offset="100%" stop-color="#00e676" stop-opacity="0.04"/></linearGradient>
  </defs>
  <!-- Client Request -->
  <rect x="130" y="10" width="220" height="36" rx="18" fill="rgba(255,255,255,0.04)" stroke="rgba(255,255,255,0.12)" stroke-width="1"/>
  <text x="240" y="33" text-anchor="middle" fill="#e4e2ec" font-size="12" font-weight="600">Client Request</text>
  <line x1="240" y1="46" x2="240" y2="70" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <!-- Parse -->
  <rect x="110" y="75" width="260" height="44" rx="8" fill="url(#c2)" stroke="#00e5ff" stroke-opacity="0.4" stroke-width="1"/>
  <text x="240" y="95" text-anchor="middle" fill="#00e5ff" font-size="11" font-weight="600">Parse query parameters</text>
  <text x="240" y="110" text-anchor="middle" fill="#56546a" font-size="9">from, to, datetime</text>
  <line x1="240" y1="119" x2="240" y2="140" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <!-- Nearest stops -->
  <rect x="110" y="145" width="260" height="44" rx="8" fill="url(#c2)" stroke="#00e5ff" stroke-opacity="0.3" stroke-width="1"/>
  <text x="240" y="165" text-anchor="middle" fill="#e4e2ec" font-size="11" font-weight="600">Find nearest stops</text>
  <text x="240" y="180" text-anchor="middle" fill="#56546a" font-size="9">origin and destination</text>
  <line x1="240" y1="189" x2="240" y2="210" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <!-- RAPTOR -->
  <rect x="110" y="215" width="260" height="50" rx="8" fill="url(#a4)" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1.5"/>
  <text x="240" y="237" text-anchor="middle" fill="#ffb800" font-size="12" font-weight="700">Run RAPTOR</text>
  <text x="240" y="255" text-anchor="middle" fill="#8b89a0" font-size="9">Collect Pareto-optimal journeys</text>
  <line x1="240" y1="265" x2="240" y2="290" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <!-- Diamond: Enough? -->
  <polygon points="240,295 310,325 240,355 170,325" fill="url(#a4)" stroke="#ffb800" stroke-opacity="0.4" stroke-width="1"/>
  <text x="240" y="329" text-anchor="middle" fill="#ffb800" font-size="9" font-weight="600">Enough?</text>
  <!-- Loop back (No) -->
  <line x1="310" y1="325" x2="410" y2="325" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1"/>
  <line x1="410" y1="325" x2="410" y2="240" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1"/>
  <line x1="410" y1="240" x2="375" y2="240" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1" marker-end="url(#a3)"/>
  <text x="425" y="285" fill="#ff5252" font-size="8" font-weight="600">NO</text>
  <text x="425" y="297" fill="#56546a" font-size="7">exclude patterns</text>
  <!-- Yes -->
  <line x1="240" y1="355" x2="240" y2="380" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <text x="255" y="372" fill="#00e676" font-size="8" font-weight="600">YES</text>
  <!-- Reconstruct -->
  <rect x="110" y="385" width="260" height="50" rx="8" fill="url(#g2)" stroke="#00e676" stroke-opacity="0.4" stroke-width="1"/>
  <text x="240" y="407" text-anchor="middle" fill="#00e676" font-size="11" font-weight="600">Reconstruct journeys</text>
  <text x="240" y="425" text-anchor="middle" fill="#56546a" font-size="9">Tag · Format Navitia response</text>
  <line x1="240" y1="435" x2="240" y2="460" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <!-- Response -->
  <rect x="155" y="465" width="170" height="36" rx="18" fill="rgba(0,230,118,0.08)" stroke="#00e676" stroke-opacity="0.3" stroke-width="1"/>
  <text x="240" y="488" text-anchor="middle" fill="#00e676" font-size="12" font-weight="600">JSON Response</text>
</svg>

### Walk / Bike / Car Journey

<svg viewBox="0 0 480 370" xmlns="http://www.w3.org/2000/svg" style="max-width:480px;width:100%;font-family:'DM Sans',sans-serif;">
  <defs>
    <marker id="a5" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#8b89a0"/></marker>
    <linearGradient id="c5" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00e5ff" stop-opacity="0.12"/><stop offset="100%" stop-color="#00e5ff" stop-opacity="0.04"/></linearGradient>
    <linearGradient id="g5" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00e676" stop-opacity="0.12"/><stop offset="100%" stop-color="#00e676" stop-opacity="0.04"/></linearGradient>
  </defs>
  <!-- Client Request -->
  <rect x="130" y="10" width="220" height="36" rx="18" fill="rgba(255,255,255,0.04)" stroke="rgba(255,255,255,0.12)" stroke-width="1"/>
  <text x="240" y="33" text-anchor="middle" fill="#e4e2ec" font-size="12" font-weight="600">Client Request</text>
  <line x1="240" y1="46" x2="240" y2="70" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a5)"/>
  <!-- Build request -->
  <rect x="110" y="75" width="260" height="44" rx="8" fill="url(#c5)" stroke="#00e5ff" stroke-opacity="0.4" stroke-width="1"/>
  <text x="240" y="95" text-anchor="middle" fill="#00e5ff" font-size="11" font-weight="600">Build Valhalla request</text>
  <text x="240" y="110" text-anchor="middle" fill="#56546a" font-size="9">costing model + options</text>
  <line x1="240" y1="119" x2="240" y2="140" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a5)"/>
  <!-- Call Valhalla -->
  <rect x="110" y="145" width="260" height="44" rx="8" fill="rgba(0,230,118,0.08)" stroke="#00e676" stroke-opacity="0.4" stroke-width="1.5"/>
  <text x="240" y="165" text-anchor="middle" fill="#00e676" font-size="11" font-weight="600">Call Valhalla /route</text>
  <text x="240" y="180" text-anchor="middle" fill="#56546a" font-size="9">HTTP on localhost:8002</text>
  <line x1="240" y1="189" x2="240" y2="210" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a5)"/>
  <!-- Process -->
  <rect x="90" y="215" width="300" height="60" rx="8" fill="url(#c5)" stroke="#00e5ff" stroke-opacity="0.3" stroke-width="1"/>
  <text x="240" y="237" text-anchor="middle" fill="#e4e2ec" font-size="11" font-weight="600">Decode polyline · Extract maneuvers</text>
  <text x="240" y="255" text-anchor="middle" fill="#56546a" font-size="9">Elevation colors (bike) · Format Navitia response</text>
  <line x1="240" y1="275" x2="240" y2="305" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a5)"/>
  <!-- Response -->
  <rect x="155" y="310" width="170" height="36" rx="18" fill="rgba(0,230,118,0.08)" stroke="#00e676" stroke-opacity="0.3" stroke-width="1"/>
  <text x="240" y="333" text-anchor="middle" fill="#00e676" font-size="12" font-weight="600">JSON Response</text>
</svg>

## Hot Reload

The hot reload mechanism allows updating GTFS data without downtime:

1. `POST /api/reload` is called (requires `api_key`)
2. A background thread loads new GTFS data and builds a fresh RAPTOR index
3. The new index is swapped in atomically via `ArcSwap`
4. All in-flight requests continue using the old index until they complete
5. The old index is dropped when the last reference is released
