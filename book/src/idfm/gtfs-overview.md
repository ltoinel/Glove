# GTFS Data Overview

Glove loads transit data in the [GTFS](https://gtfs.org/) format (General Transit Feed Specification), an open standard used worldwide by transit agencies to describe their schedules, stops, and routes. The data is published by [Ile-de-France Mobilités](https://data.iledefrance-mobilites.fr/) (IDFM), the public transit authority for the Paris region.

## What is GTFS?

GTFS is a collection of CSV files that together describe a complete transit network. Think of it as a structured way to answer: *"What vehicles go where, at what time, and how do passengers connect between them?"*

```admonish info title="How Glove uses GTFS"
All GTFS data is loaded into memory at startup. There is no database — the entire transit network lives in RAM for maximum query speed. This is what makes Glove fast: a journey query scans in-memory data structures instead of hitting a disk.
```

## Dataset at a Glance

| File | Records | What it contains |
|------|--------:|-----------------|
| **agencies** | 61 | Transit operators (RATP, SNCF, local bus companies...) |
| **routes** | 2,009 | Transit lines — each bus line, metro line, or RER line is a route |
| **stops** | 54,011 | Physical locations where passengers board or alight |
| **trips** | 495,345 | Individual vehicle runs — one bus doing its morning route is one trip |
| **stop_times** | 10,933,796 | The schedule: what time each trip arrives/departs at each stop |
| **transfers** | 206,822 | Walking connections between nearby stops (for changing lines) |
| **calendars** | 1,169 | Service patterns: which days each schedule runs (weekdays, weekends...) |
| **calendar_dates** | 2,937 | Exceptions to the calendar (holidays, strikes, special events...) |

### Understanding the scale

To put these numbers in perspective:

- **10.9 million stop times** means the dataset contains nearly 11 million individual "a vehicle stops here at this time" records. This is the bulk of the data.
- **495,345 trips** represent every individual vehicle departure across all lines, all day, all week. A single metro line might have hundreds of trips per day.
- **206,822 transfers** define where passengers can walk between stops to change lines. For example, walking from a metro platform to a nearby bus stop.

## Understanding GTFS Objects

### Agencies

An **agency** is a transit operator. Ile-de-France has 61 agencies, from large operators like RATP (Paris metro, buses, tramways) to small local bus companies covering specific towns.

### Routes

A **route** is a transit line as passengers know it — "Metro line 4", "Bus 72", or "RER A". Each route has:
- A **short name** displayed on vehicles and maps (e.g. "4", "72", "A")
- A **type** indicating the transport mode (metro, bus, rail, tramway...)
- A **color** for visual identification

### Stops

A **stop** is a physical place where passengers board or leave a vehicle. GTFS has three levels:
- **Stop points** (36,126) — the actual boarding location, like a specific platform or bus bay. "RER A, platform 1, Gare de Lyon" is a stop point.
- **Stations** (15,369) — a group of stop points. "Gare de Lyon" is a station that contains stop points for RER A, RER D, metro 1, metro 14, and several bus lines.
- **Entrances** (2,516) — physical entry/exit points to a station, like a specific stairway or elevator from street level.

### Trips

A **trip** is one vehicle making one run along a route. For example, "the metro line 4 departing Porte de Clignancourt at 08:15" is a trip. The same route has hundreds of trips per day, one for each departure.

### Stop Times

A **stop time** is the scheduled arrival and departure of a trip at a specific stop. It's the core of the timetable: "trip T123 arrives at Châtelet at 08:22 and departs at 08:23". With nearly 11 million of these, the dataset covers every scheduled stop of every vehicle.

### Transfers

A **transfer** defines a walking connection between two stops, with a minimum transfer time in seconds. This tells the routing algorithm: "a passenger can walk from stop A to stop B in 120 seconds to change lines." Without transfers, the algorithm wouldn't know that two stops are close enough to walk between.

### Calendars & Calendar Dates

**Calendars** define weekly patterns: "this schedule runs Monday to Friday" or "weekends and holidays only." **Calendar dates** are exceptions: "this schedule does NOT run on December 25" or "this special schedule runs on July 14." Together, they let Glove know which trips are active on any given date.

## Routes by Transport Mode

| Mode | GTFS Type | Routes | Description |
|------|----------:|-------:|-------------|
| Bus | 3 | 1,950 | Urban and suburban bus lines |
| Rail | 2 | 24 | RER, Transilien, and TER regional trains |
| Tramway | 0 | 17 | Tramway lines and automated shuttles |
| Métro | 1 | 16 | Paris underground metro |
| Funiculaire | 7 | 1 | Montmartre funicular |
| Navette | 6 | 1 | Automated shuttle |

Bus routes represent **97%** of all routes by count, but metro and RER carry the majority of daily passengers. Glove routes across all modes.

## Métro Lines

16 metro lines operated by RATP, forming the backbone of urban transit in Paris:

| Line | | Line | | Line | | Line |
|------|--|------|--|------|--|------|
| **1** | | **5** | | **9** | | **13** |
| **2** | | **6** | | **10** | | **14** |
| **3** | | **7** | | **11** | | **3B** |
| **4** | | **8** | | **12** | | **7B** |

Lines 3B and 7B are short branch lines. Line 14 is fully automated and the newest, recently extended to Orly airport.

## RER Lines

5 RER (Réseau Express Régional) lines cross Paris and connect the city center to the suburbs and airports:

| Line | Terminals |
|------|-----------|
| **A** | Saint-Germain / Cergy / Poissy — Marne-la-Vallée / Boissy |
| **B** | CDG Airport / Mitry — Robinson / Saint-Rémy |
| **C** | Versailles / Saint-Quentin — Dourdan / Saint-Martin-d'Étampes |
| **D** | Orry-la-Ville / Creil — Melun / Malesherbes / Corbeil |
| **E** | Haussmann — Chelles / Tournan / Mantes (via Nanterre) |

RER lines are heavier, faster, and cover longer distances than the metro. They are the primary way to reach airports (CDG via RER B, Orly via RER C) and major suburban hubs.

## Transilien Lines

9 suburban rail lines operated by SNCF, serving the outer suburbs:

| Line | | Line | | Line |
|------|--|------|--|------|
| **H** | | **L** | | **R** |
| **J** | | **N** | | **U** |
| **K** | | **P** | | **V** |

Transilien lines depart from the major Paris train stations (Saint-Lazare, Gare du Nord, Gare de Lyon, Montparnasse, Gare de l'Est) and serve towns beyond the reach of metro and RER.

## TER Lines

Regional trains (Train Express Régional) from neighboring regions that serve stations within Ile-de-France:

- TER Bourgogne - Franche-Comté
- TER Centre - Val de Loire
- TER Hauts-de-France
- TER Normandie
- TER Grand-Est

These are operated by SNCF for the respective regions and provide connections to cities outside Ile-de-France.

## Tramway Lines

17 tramway and automated lines, mostly running in the inner suburbs:

| Line | | Line | | Line |
|------|--|------|--|------|
| **T1** | | **T6** | | **T11** |
| **T2** | | **T7** | | **T12** |
| **T3a** | | **T8** | | **T13** |
| **T3b** | | **T9** | | **T14** |
| **T4** | | **T10** | | **CDG VAL** |
| **T5** | | | | **ORLYVAL** |

T3a and T3b form a ring around Paris along the boulevards des Maréchaux. CDG VAL and ORLYVAL are automated airport shuttles.

## Stops Hierarchy

GTFS organizes stops in a three-level hierarchy:

| Level | Count | What it represents |
|-------|------:|-------------------|
| **Stop points** | 36,126 | The exact spot where you board — a platform, a bus bay, a specific door |
| **Stations** | 15,369 | A named place grouping multiple stop points — "Gare du Nord" contains platforms for metro 4, metro 5, RER B, RER D, RER E, Transilien H, and bus stops |
| **Entrances** | 2,516 | Physical ways into a station — a stairway, an elevator, a specific street-level door |

This hierarchy is important for routing: when you search for "Gare du Nord", Glove finds the station and then considers all its stop points to find the best boarding platform for your journey.

## Top 15 Transit Operators

| Operator | Routes | Coverage |
|----------|-------:|----------|
| RATP | 246 | Metro, RER A/B, tramways, Paris buses |
| Centre et Sud Yvelines | 109 | Bus network in southern Yvelines |
| Poissy - Les Mureaux | 88 | Bus network in northern Yvelines |
| Coeur d'Essonne | 66 | Bus network in central Essonne |
| Mantois | 64 | Bus network around Mantes-la-Jolie |
| Brie et 2 Morin | 64 | Bus network in eastern Seine-et-Marne |
| Roissy Ouest | 63 | Bus network near CDG airport |
| Meaux et Ourcq | 55 | Bus network around Meaux |
| Pays Briard | 55 | Bus network in southern Seine-et-Marne |
| Paris Saclay | 53 | Bus network around the Saclay plateau |
| Argenteuil - Boucles de Seine | 53 | Bus network in northern Hauts-de-Seine |
| Saint-Quentin-en-Yvelines | 51 | Bus network around SQY |
| Provinois - Brie et Seine | 48 | Bus network in far eastern Seine-et-Marne |
| Fontainebleau - Moret | 47 | Bus network around Fontainebleau |
| Essonne Sud Ouest | 47 | Bus network in southwestern Essonne |

RATP is by far the largest operator, running all metro lines, RER A and B, most tramway lines, and a massive bus network covering Paris and the near suburbs. The remaining 60 operators are organized by geographic zone and operate bus-only networks.

## Data Source

The GTFS dataset is downloaded from:

```
https://data.iledefrance-mobilites.fr/explore/dataset/offre-horaires-tc-gtfs-idfm/
```

It is updated regularly by IDFM (typically every few weeks when schedules change). Glove can hot-reload the data via `POST /api/reload` without service interruption — the new RAPTOR index is built in a background thread and swapped in atomically.
