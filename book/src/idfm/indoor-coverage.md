# Indoor Routing Coverage

This report analyses the availability of indoor pedestrian routing data in Valhalla tiles for GTFS transfer pairs in Ile-de-France. Indoor maneuvers (elevators, stairs, escalators, building entrances/exits) enable accurate turn-by-turn instructions for station transfers.

```admonish info title="How this report was generated"
The `scripts/check_indoor.py` script queries Valhalla's pedestrian route API for each unique GTFS transfer pair and checks whether the response contains indoor maneuver types (39-43). See [Development Setup](../contributing/development.md) for usage.
```

```admonish warning title="Figures from an earlier dataset"
The figures on this page were produced in April 2026 (first published 2026-04-07), on the IDFM GTFS and OSM extract of that time. They have not been regenerated since: the dataset loaded on 2026-09-28 has 192,366 rows in `transfers.txt`, which make **96,240 unique transfer pairs** (against 71,479 here). Treat the proportions as indicative, and regenerate the report (see the end of this page) before relying on exact counts.
```

## Summary

| Metric | Value |
|--------|-------|
| Total transfer pairs checked | 71,479 |
| With indoor routing data | 3,729 (5.2%) |
| Outdoor only | 67,745 (94.8%) |
| Routing errors | 5 |
| Stations with indoor data | 355 / 9,047 (3.9%) |

## Indoor Maneuver Types

| Type | Count | Description |
|------|-------|-------------|
| Escalator | 3,060 | Escalator transitions between levels |
| Stairs | 1,441 | Staircase transitions |
| Elevator | 1,296 | Elevator/lift transitions |
| Enter building | 34 | Entering a station building |
| Exit building | 26 | Exiting a station building |

Escalators are the most commonly mapped indoor element, followed by stairs and elevators. Building entrance/exit transitions are rare, found mainly at CDG airport terminals.

## Top 25 Stations by Indoor Coverage

Stations ranked by **indoor score**: the total count of indoor maneuvers (elevators, stairs, escalators, building enter/exit) across all transfer pairs involving the station. A pair whose two stops belong to the same station counts twice for it.

| Station | Score | Ratio | Elevators | Stairs | Escalators |
|---------|------:|------:|----------:|-------:|-----------:|
| Gare Saint-Lazare | 1,344 | 53% | 85 | 324 | 935 |
| La Défense | 1,089 | 57% | 47 | 0 | 1,042 |
| Gare du Nord | 646 | 52% | 8 | 313 | 325 |
| Massy - Palaiseau | 633 | 87% | 229 | 0 | 404 |
| Gare Montparnasse | 527 | 27% | 0 | 48 | 479 |
| Versailles Chantiers | 478 | 77% | 140 | 0 | 338 |
| Opera | 384 | 34% | 0 | 228 | 156 |
| Republique | 332 | 32% | 0 | 288 | 44 |
| Juvisy | 304 | 74% | 242 | 0 | 62 |
| Gare de l'Est | 260 | 32% | 8 | 88 | 164 |
| Saint-Quentin en Yvelines | 243 | 61% | 145 | 0 | 98 |
| Gare de Lyon | 226 | 25% | 10 | 65 | 151 |
| Europe | 210 | 32% | 0 | 134 | 76 |
| Corbeil-Essonnes | 206 | 58% | 8 | 198 | 0 |
| Bibliotheque Francois Mitterrand | 198 | 50% | 29 | 3 | 166 |
| Aeroport CDG Terminal 2 (TGV) | 180 | 68% | 36 | 0 | 66 |
| Fontainebleau - Avon | 164 | 74% | 137 | 27 | 0 |
| Aulnay-sous-Bois | 140 | 59% | 140 | 0 | 0 |
| Neuilly - Porte Maillot | 140 | 18% | 0 | 32 | 108 |
| Magenta | 126 | 75% | 4 | 13 | 109 |
| Haussmann Saint-Lazare | 119 | 100% | 37 | 16 | 66 |
| Evry - Courcouronnes | 87 | 28% | 21 | 0 | 66 |
| Havre - Caumartin | 81 | 10% | 16 | 21 | 44 |
| Bondy | 75 | 59% | 8 | 67 | 0 |
| Rosny-sous-Bois | 64 | 31% | 0 | 64 | 0 |

## Key Observations

### Well-covered stations

The major railway hubs have the best indoor coverage:

- **Gare Saint-Lazare** has the highest indoor score (1,344) with a good mix of escalators, stairs, and elevators. Over half of its transfer pairs include indoor maneuvers.
- **La Defense** scores second (1,089), dominated by escalators — consistent with its large underground hub connecting RER, metro, and tramway.
- **Gare du Nord** and **Gare Montparnasse** are well mapped, with escalators and stairs.
- **Haussmann Saint-Lazare** achieves a 100% indoor ratio — all its transfer pairs return indoor maneuvers.

### RER/Transilien stations

Several suburban stations have high indoor ratios thanks to elevator mappings:

- **Massy-Palaiseau** (87%), **Versailles Chantiers** (77%), **Juvisy** (74%), **Fontainebleau-Avon** (74%) — these stations likely have well-mapped accessibility elevators in OSM.
- **Aulnay-sous-Bois** has 140 elevator maneuvers, suggesting comprehensive accessibility mapping.

### Metro stations

Metro stations have lower coverage. **Opera** (34%), **Republique** (32%), and **Europe** (32%) appear in the top 25 but with moderate ratios. Most metro stations have no indoor data — the corridors and stairs connecting platforms are not yet mapped in OSM.

### Gaps

- **94.8%** of transfer pairs have no indoor routing data
- Only **3.9%** of stations (355/9,047) have any indoor coverage
- Building enter/exit maneuvers are almost exclusively at **CDG airport**
- Most central Paris metro interchange stations (Chatelet, Nation, Strasbourg Saint-Denis...) are missing from the indoor data

## How Glove Uses This Data

Every transfer section of a public transport journey is routed through Valhalla, whether or not indoor data exists:

- A transfer between two stops of the **same parent station** is tagged `transfer_type: "indoor"` and routed with **zero step and elevator penalties**, so stairs, escalators and lifts are used freely inside the station. Any other transfer is tagged `"outdoor"` and uses the configured pedestrian penalties.
- The Valhalla shape and distance are attached to the section in both cases. Where OSM has no indoor ways, that shape follows the nearest mapped paths — often the street — so its geometry is only as good as the local mapping.
- Turn-by-turn **maneuvers** are included only when `routing.maneuvers: true` in `config.yaml` (server-controlled, `false` by default), and then for every transfer, indoor data or not.

The portal draws every transfer's shape on the map, or a straight line between the two stops when Valhalla returned none. When maneuvers are present, indoor ones (elevator, stairs, escalator) are placed as markers along the shape. The indoor coverage measured above therefore decides how realistic the drawn transfer is, not whether one is drawn.

## Improving Coverage

Indoor coverage depends on OSM contributors mapping station interiors. Key tags to add:

| OSM Tag | Description |
|---------|-------------|
| `highway=footway` | Indoor walkways/corridors |
| `highway=steps` | Staircases between levels |
| `highway=elevator` | Elevators/lifts |
| `indoor=yes` | Marks a way as indoor |
| `level=*` | Floor level (-1, 0, 1, ...) |
| `railway=subway_entrance` | Metro station entrance nodes |

Tools for editing indoor data:
- [OpenLevelUp](https://openlevelup.net/) — visualize existing indoor data by level
- [JOSM](https://josm.openstreetmap.de/) with IndoorHelper plugin — edit indoor features
- [Overpass Turbo](https://overpass-turbo.eu/) — query indoor tags around a station

```admonish tip title="Regenerate this report"
After updating Valhalla tiles with new OSM data, regenerate the CSV files:

~~~bash
python3 scripts/check_indoor.py --output data/indoor_report.csv --summary data/indoor_summary.csv
~~~
```
