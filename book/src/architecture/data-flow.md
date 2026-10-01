# Data Flow

## Startup Sequence

<svg viewBox="0 0 560 620" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Glove startup sequence" style="max-width:560px;width:100%;font-family:'Inter',-apple-system,BlinkMacSystemFont,sans-serif;">
  <defs>
    <marker id="a1" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="#62607a"/></marker>
    <linearGradient id="cg" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#22d3ee" stop-opacity="0.16"/><stop offset="100%" stop-color="#818cf8" stop-opacity="0.06"/></linearGradient>
    <linearGradient id="ag" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#fbbf24" stop-opacity="0.18"/><stop offset="100%" stop-color="#fbbf24" stop-opacity="0.05"/></linearGradient>
    <linearGradient id="gg" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#34d399" stop-opacity="0.18"/><stop offset="100%" stop-color="#34d399" stop-opacity="0.05"/></linearGradient>
  </defs>
  <!-- config.yaml -->
  <rect x="200" y="10" width="160" height="40" rx="9" fill="rgba(255,255,255,0.04)" stroke="rgba(255,255,255,0.14)"/>
  <text x="280" y="35" text-anchor="middle" fill="#e7e7f0" font-size="12" font-weight="600">config.yaml</text>
  <line x1="280" y1="50" x2="280" y2="76" stroke="#62607a" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- Load Config -->
  <rect x="178" y="80" width="204" height="44" rx="9" fill="url(#cg)" stroke="#22d3ee" stroke-opacity="0.45"/>
  <text x="280" y="107" text-anchor="middle" fill="#67e8f9" font-size="12" font-weight="700">Load config → check cache</text>
  <line x1="280" y1="124" x2="280" y2="150" stroke="#62607a" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- Diamond: Cache valid? -->
  <polygon points="280,153 344,186 280,219 216,186" fill="url(#ag)" stroke="#fbbf24" stroke-opacity="0.6"/>
  <text x="280" y="190" text-anchor="middle" fill="#fbbf24" font-size="10" font-weight="700">cache valid?</text>
  <!-- Yes branch (left) -->
  <line x1="216" y1="186" x2="124" y2="186" stroke="#62607a" stroke-width="1" marker-end="url(#a1)"/>
  <text x="170" y="178" text-anchor="middle" fill="#34d399" font-size="9" font-weight="700">YES</text>
  <rect x="18" y="164" width="106" height="44" rx="9" fill="url(#gg)" stroke="#34d399" stroke-opacity="0.45"/>
  <text x="71" y="183" text-anchor="middle" fill="#34d399" font-size="10.5" font-weight="700">Load cache</text>
  <text x="71" y="198" text-anchor="middle" fill="#62607a" font-size="9">sub-second</text>
  <!-- No branch (right) -->
  <line x1="344" y1="186" x2="436" y2="186" stroke="#62607a" stroke-width="1" marker-end="url(#a1)"/>
  <text x="390" y="178" text-anchor="middle" fill="#ff5252" font-size="9" font-weight="700">NO</text>
  <rect x="436" y="160" width="116" height="56" rx="9" fill="url(#ag)" stroke="#fbbf24" stroke-opacity="0.45"/>
  <text x="494" y="182" text-anchor="middle" fill="#fbbf24" font-size="10.5" font-weight="700">Parse GTFS</text>
  <text x="494" y="197" text-anchor="middle" fill="#9b9ab2" font-size="9">build RAPTOR + cache</text>
  <text x="494" y="210" text-anchor="middle" fill="#62607a" font-size="8">10-30 seconds</text>
  <!-- Merge -->
  <line x1="71" y1="208" x2="71" y2="252" stroke="#62607a" stroke-width="1"/>
  <line x1="494" y1="216" x2="494" y2="252" stroke="#62607a" stroke-width="1"/>
  <line x1="71" y1="252" x2="494" y2="252" stroke="#62607a" stroke-width="1"/>
  <line x1="280" y1="252" x2="280" y2="274" stroke="#62607a" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- BAN -->
  <rect x="178" y="278" width="204" height="44" rx="9" fill="url(#cg)" stroke="#818cf8" stroke-opacity="0.4"/>
  <text x="280" y="298" text-anchor="middle" fill="#a5b4fc" font-size="12" font-weight="700">Load BAN data</text>
  <text x="280" y="314" text-anchor="middle" fill="#62607a" font-size="10">address index · cache or CSV</text>
  <line x1="280" y1="322" x2="280" y2="344" stroke="#62607a" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- Tile dir -->
  <rect x="178" y="348" width="204" height="40" rx="9" fill="rgba(255,255,255,0.04)" stroke="rgba(255,255,255,0.14)"/>
  <text x="280" y="366" text-anchor="middle" fill="#e7e7f0" font-size="11.5" font-weight="600">Create tile cache dir</text>
  <text x="280" y="380" text-anchor="middle" fill="#62607a" font-size="9">data/tiles/</text>
  <line x1="280" y1="388" x2="280" y2="410" stroke="#62607a" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- Background services -->
  <rect x="10" y="414" width="540" height="110" rx="12" fill="rgba(255,255,255,0.02)" stroke="rgba(255,255,255,0.10)" stroke-dasharray="4 4"/>
  <text x="280" y="432" text-anchor="middle" fill="#9b9ab2" font-size="10" font-weight="700">Wire shared state (before binding)</text>
  <rect x="22" y="442" width="164" height="70" rx="9" fill="url(#ag)" stroke="#fbbf24" stroke-opacity="0.4"/>
  <text x="104" y="464" text-anchor="middle" fill="#fbbf24" font-size="11" font-weight="700">Road traffic</text>
  <text x="104" y="480" text-anchor="middle" fill="#9b9ab2" font-size="9">parse Sytadin geometry</text>
  <text x="104" y="494" text-anchor="middle" fill="#9b9ab2" font-size="9">start states poller</text>
  <text x="104" y="506" text-anchor="middle" fill="#62607a" font-size="8">if traffic.enabled</text>
  <rect x="198" y="442" width="164" height="70" rx="9" fill="url(#ag)" stroke="#fbbf24" stroke-opacity="0.4"/>
  <text x="280" y="464" text-anchor="middle" fill="#fbbf24" font-size="11" font-weight="700">Real-time service</text>
  <text x="280" y="480" text-anchor="middle" fill="#9b9ab2" font-size="9">one poller per GTFS-RT feed</text>
  <text x="280" y="494" text-anchor="middle" fill="#9b9ab2" font-size="9">reads the RAPTOR ArcSwap</text>
  <text x="280" y="506" text-anchor="middle" fill="#62607a" font-size="8">if realtime.enabled</text>
  <rect x="374" y="442" width="164" height="70" rx="9" fill="url(#ag)" stroke="#fbbf24" stroke-opacity="0.4"/>
  <text x="456" y="464" text-anchor="middle" fill="#fbbf24" font-size="11" font-weight="700">Disruption catalog</text>
  <text x="456" y="480" text-anchor="middle" fill="#9b9ab2" font-size="9">load disruptions.json</text>
  <text x="456" y="494" text-anchor="middle" fill="#9b9ab2" font-size="9">into an ArcSwap</text>
  <text x="456" y="506" text-anchor="middle" fill="#62607a" font-size="8">{data.dir}/disruptions/</text>
  <line x1="280" y1="524" x2="280" y2="550" stroke="#62607a" stroke-width="1.5" marker-end="url(#a1)"/>
  <!-- Actix -->
  <rect x="148" y="554" width="264" height="52" rx="11" fill="url(#gg)" stroke="#34d399" stroke-opacity="0.5"/>
  <text x="280" y="577" text-anchor="middle" fill="#34d399" font-size="13" font-weight="800">Start Actix-web</text>
  <text x="280" y="595" text-anchor="middle" fill="#9b9ab2" font-size="10">serve REST API · :8080</text>
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
| `pathways.txt` (optional) | In-station walkways, used to time transfers within a station | `Pathway` |

## Query Flow

### Public Transit Journey

<svg viewBox="0 0 540 920" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Public transport journey query flow" style="max-width:540px;width:100%;font-family:'DM Sans',sans-serif;">
  <defs>
    <marker id="a2" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#8b89a0"/></marker>
    <marker id="a3" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#ffb800"/></marker>
    <linearGradient id="c2" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00e5ff" stop-opacity="0.12"/><stop offset="100%" stop-color="#00e5ff" stop-opacity="0.04"/></linearGradient>
    <linearGradient id="a4" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#ffb800" stop-opacity="0.15"/><stop offset="100%" stop-color="#ffb800" stop-opacity="0.05"/></linearGradient>
    <linearGradient id="g2" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00e676" stop-opacity="0.12"/><stop offset="100%" stop-color="#00e676" stop-opacity="0.04"/></linearGradient>
  </defs>
  <!-- Client Request -->
  <rect x="150" y="10" width="220" height="36" rx="18" fill="rgba(255,255,255,0.04)" stroke="rgba(255,255,255,0.12)" stroke-width="1"/>
  <text x="260" y="33" text-anchor="middle" fill="#e4e2ec" font-size="12" font-weight="600">Client Request</text>
  <line x1="260" y1="46" x2="260" y2="70" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <rect x="110" y="75" width="300" height="44" rx="8" fill="url(#c2)" stroke="#00e5ff" stroke-opacity="0.4" stroke-width="1"/>
  <text x="260" y="94" text-anchor="middle" fill="#00e5ff" font-size="11" font-weight="600">Resolve endpoints within radius</text>
  <text x="260" y="109" text-anchor="middle" fill="#56546a" font-size="9">stop id → station platforms · address → nearby stops</text>
  <line x1="260" y1="119" x2="260" y2="140" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <rect x="110" y="145" width="300" height="44" rx="8" fill="url(#c2)" stroke="#00e5ff" stroke-opacity="0.3" stroke-width="1"/>
  <text x="260" y="164" text-anchor="middle" fill="#e4e2ec" font-size="11" font-weight="600">Time address walks with Valhalla</text>
  <text x="260" y="179" text-anchor="middle" fill="#56546a" font-size="9">pedestrian matrix · routing.walk_matrix_stops</text>
  <line x1="260" y1="189" x2="260" y2="210" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <rect x="110" y="215" width="300" height="44" rx="8" fill="url(#c2)" stroke="#00e5ff" stroke-opacity="0.3" stroke-width="1"/>
  <text x="260" y="234" text-anchor="middle" fill="#e4e2ec" font-size="11" font-weight="600">Check prefer_rail reachability</text>
  <text x="260" y="249" text-anchor="middle" fill="#56546a" font-size="9">rail/metro/tram within prefer_rail_max_walk at both ends</text>
  <line x1="260" y1="259" x2="260" y2="285" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <!-- Blocking pool -->
  <rect x="70" y="290" width="400" height="322" rx="12" fill="rgba(255,255,255,0.02)" stroke="rgba(255,255,255,0.12)" stroke-dasharray="4 4"/>
  <text x="270" y="306" text-anchor="middle" fill="#8b89a0" font-size="9" font-weight="600">web::block — off the async executor</text>
  <rect x="110" y="316" width="300" height="40" rx="8" fill="url(#c2)" stroke="#00e5ff" stroke-opacity="0.3" stroke-width="1"/>
  <text x="260" y="333" text-anchor="middle" fill="#e4e2ec" font-size="11" font-weight="600">Exclude forbidden modes + blocked lines</text>
  <text x="260" y="347" text-anchor="middle" fill="#56546a" font-size="9">forbidden_modes · disruptions in force at the query instant</text>
  <line x1="260" y1="356" x2="260" y2="380" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <rect x="110" y="385" width="300" height="50" rx="8" fill="url(#a4)" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1.5"/>
  <text x="260" y="406" text-anchor="middle" fill="#ffb800" font-size="12" font-weight="700">Run RAPTOR → build journeys</text>
  <text x="260" y="424" text-anchor="middle" fill="#8b89a0" font-size="9">rail tier first when prefer_rail (buses forbidden)</text>
  <line x1="260" y1="435" x2="260" y2="455" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <!-- Diamond: Enough? -->
  <polygon points="260,458 330,490 260,522 190,490" fill="url(#a4)" stroke="#ffb800" stroke-opacity="0.4" stroke-width="1"/>
  <text x="260" y="494" text-anchor="middle" fill="#ffb800" font-size="9" font-weight="600">Enough?</text>
  <!-- Loop back (No) -->
  <line x1="330" y1="490" x2="440" y2="490" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1"/>
  <line x1="440" y1="490" x2="440" y2="410" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1"/>
  <line x1="440" y1="410" x2="414" y2="410" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1" marker-end="url(#a3)"/>
  <text x="446" y="436" fill="#ff5252" font-size="8" font-weight="600">NO</text>
  <text x="446" y="448" fill="#56546a" font-size="7.5">exclude used</text>
  <text x="446" y="458" fill="#56546a" font-size="7.5">patterns (+ head</text>
  <text x="446" y="468" fill="#56546a" font-size="7.5">line if diverse_lines)</text>
  <!-- Yes -->
  <line x1="260" y1="522" x2="260" y2="548" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <text x="272" y="540" fill="#00e676" font-size="8" font-weight="600">YES</text>
  <rect x="110" y="553" width="300" height="44" rx="8" fill="url(#a4)" stroke="#ffb800" stroke-opacity="0.3" stroke-width="1"/>
  <text x="260" y="572" text-anchor="middle" fill="#ffb800" font-size="11" font-weight="600">Blocked-alternative pass</text>
  <text x="260" y="587" text-anchor="middle" fill="#56546a" font-size="9">only if a blocking disruption is in force → status: blocked</text>
  <line x1="260" y1="612" x2="260" y2="634" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <!-- Diamond: nothing found? -->
  <polygon points="260,637 340,668 260,699 180,668" fill="url(#a4)" stroke="#ffb800" stroke-opacity="0.4" stroke-width="1"/>
  <text x="260" y="665" text-anchor="middle" fill="#ffb800" font-size="8.5" font-weight="600">nothing found</text>
  <text x="260" y="677" text-anchor="middle" fill="#ffb800" font-size="8.5" font-weight="600">&amp; address?</text>
  <!-- Retry (Yes) -->
  <line x1="180" y1="668" x2="40" y2="668" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1"/>
  <line x1="40" y1="668" x2="40" y2="97" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1"/>
  <line x1="40" y1="97" x2="106" y2="97" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1" marker-end="url(#a3)"/>
  <text x="172" y="687" text-anchor="end" fill="#00e676" font-size="8" font-weight="600">YES · retry once with</text>
  <text x="172" y="698" text-anchor="end" fill="#56546a" font-size="7.5">fallback_stop_distance</text>
  <!-- No -->
  <line x1="260" y1="699" x2="260" y2="725" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <text x="272" y="716" fill="#ff5252" font-size="8" font-weight="600">NO</text>
  <rect x="110" y="730" width="300" height="44" rx="8" fill="url(#g2)" stroke="#00e676" stroke-opacity="0.4" stroke-width="1"/>
  <text x="260" y="749" text-anchor="middle" fill="#00e676" font-size="11" font-weight="600">Valhalla enrichment</text>
  <text x="260" y="764" text-anchor="middle" fill="#56546a" font-size="9">first/last-mile walks · transfer shapes</text>
  <line x1="260" y1="774" x2="260" y2="795" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <rect x="110" y="800" width="300" height="44" rx="8" fill="url(#g2)" stroke="#00e676" stroke-opacity="0.4" stroke-width="1"/>
  <text x="260" y="819" text-anchor="middle" fill="#00e676" font-size="11" font-weight="600">Sort by duration → tag journeys</text>
  <text x="260" y="834" text-anchor="middle" fill="#56546a" font-size="9">fastest · least_transfers · least_walking · least_waiting</text>
  <line x1="260" y1="844" x2="260" y2="866" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a2)"/>
  <!-- Response -->
  <rect x="175" y="871" width="170" height="36" rx="18" fill="rgba(0,230,118,0.08)" stroke="#00e676" stroke-opacity="0.3" stroke-width="1"/>
  <text x="260" y="894" text-anchor="middle" fill="#00e676" font-size="12" font-weight="600">JSON Response</text>
</svg>

### Walk / Bike / Car Journey

<svg viewBox="0 0 480 370" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Walk, bike and car query flow" style="max-width:480px;width:100%;font-family:'DM Sans',sans-serif;">
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
  <text x="240" y="110" text-anchor="middle" fill="#56546a" font-size="9">pedestrian · bicycle (city, ebike, road) · auto</text>
  <line x1="240" y1="119" x2="240" y2="140" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a5)"/>
  <!-- Call Valhalla -->
  <rect x="110" y="145" width="260" height="44" rx="8" fill="rgba(0,230,118,0.08)" stroke="#00e676" stroke-opacity="0.4" stroke-width="1.5"/>
  <text x="240" y="165" text-anchor="middle" fill="#00e676" font-size="11" font-weight="600">Call Valhalla /route</text>
  <text x="240" y="180" text-anchor="middle" fill="#56546a" font-size="9">valhalla.host:port (default localhost:8002)</text>
  <line x1="240" y1="189" x2="240" y2="210" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a5)"/>
  <!-- Process -->
  <rect x="90" y="215" width="300" height="60" rx="8" fill="url(#c5)" stroke="#00e5ff" stroke-opacity="0.3" stroke-width="1"/>
  <text x="240" y="237" text-anchor="middle" fill="#e4e2ec" font-size="11" font-weight="600">Encoded shape · maneuvers if routing.maneuvers</text>
  <text x="240" y="255" text-anchor="middle" fill="#56546a" font-size="9">Bike: 3 profiles in parallel · decode shape → /height elevation</text>
  <line x1="240" y1="275" x2="240" y2="305" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#a5)"/>
  <!-- Response -->
  <rect x="155" y="310" width="170" height="36" rx="18" fill="rgba(0,230,118,0.08)" stroke="#00e676" stroke-opacity="0.3" stroke-width="1"/>
  <text x="240" y="333" text-anchor="middle" fill="#00e676" font-size="12" font-weight="600">JSON Response</text>
</svg>

## Hot Reload

The hot reload mechanism allows updating GTFS data without downtime:

1. `POST /api/gtfs/reload` is called with the `X-Api-Key` header (disabled when `server.api_key` is empty)
2. On the blocking thread pool (`web::block`), the new GTFS data is loaded, a fresh RAPTOR index is built and the on-disk cache is rewritten; the HTTP response is sent once this completes
3. The new index is swapped in atomically via `ArcSwap`
4. All in-flight requests continue using the old index until they complete
5. The old index is dropped when the last reference is released

The real-time service holds the same `ArcSwap`, so it resolves its feeds against the new index on its next refresh.

## Transfer Enrichment

Transfer sections in public transport journeys are always enriched with Valhalla pedestrian routes, whatever the endpoints (stop ids or addresses). The transfer type is set from `parent_station`:

- **Outdoor transfers** (stops with different or no `parent_station`): routed with the regular walker's costing.
- **Indoor transfers** (stops sharing the same `parent_station`): routed with zero step and elevator penalties and tunnels preferred, so an in-station walk is not pushed into the street (a wheelchair request keeps its own costing).
- **Polyline rendering**: transfer sections carry the Valhalla shape and distance when Valhalla answers; otherwise the map draws a straight line between the two stops.

Identical transfers repeated across alternatives are requested once, and the distinct requests run concurrently through `futures::join_all`. `routing.maneuvers` in `config.yaml` (off by default) only controls whether the turn-by-turn maneuvers are attached to each transfer section; the shape and distance are attached either way.

For an address origin or destination, the first- and last-mile walks are fetched from Valhalla in the same enrichment step.
