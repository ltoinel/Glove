# GTFS Data Overview

Glove loads GTFS data published by [Ile-de-France Mobilités](https://data.iledefrance-mobilites.fr/) (IDFM), the public transit authority for the Paris region. This page summarizes the content and scale of the dataset.

## Dataset Summary

| Metric | Count |
|--------|------:|
| Agencies | 61 |
| Routes | 2,009 |
| Stops | 54,011 |
| Trips | 495,345 |
| Stop times | 10,933,796 |
| Transfers | 206,822 |
| Calendars | 1,169 |
| Calendar dates | 2,937 |

## Routes by Transport Mode

| Mode | GTFS Type | Routes |
|------|----------:|-------:|
| Bus | 3 | 1,950 |
| Rail (RER / Transilien / TER) | 2 | 24 |
| Tramway | 0 | 17 |
| Métro | 1 | 16 |
| Funiculaire | 7 | 1 |
| Navette | 6 | 1 |

Bus routes represent **97%** of all routes, but rail and metro carry the majority of passengers. Glove routes across all modes.

## Métro Lines

16 lines operated by RATP:

| Line | | Line | | Line | | Line |
|------|--|------|--|------|--|------|
| **1** | | **5** | | **9** | | **13** |
| **2** | | **6** | | **10** | | **14** |
| **3** | | **7** | | **11** | | **3B** |
| **4** | | **8** | | **12** | | **7B** |

## RER Lines

5 RER lines crossing Paris and the suburbs:

| Line | Description |
|------|-------------|
| **A** | Saint-Germain / Cergy / Poissy — Marne-la-Vallée / Boissy |
| **B** | CDG Airport / Mitry — Robinson / Saint-Rémy |
| **C** | Versailles / Saint-Quentin — Dourdan / Saint-Martin-d'Étampes |
| **D** | Orry-la-Ville / Creil — Melun / Malesherbes / Corbeil |
| **E** | Haussmann — Chelles / Tournan / Mantes (via Nanterre) |

## Transilien Lines

9 suburban rail lines:

| Line | | Line | | Line |
|------|--|------|--|------|
| **H** | | **L** | | **R** |
| **J** | | **N** | | **U** |
| **K** | | **P** | | **V** |

## TER Lines

Regional trains serving Ile-de-France from neighboring regions:

- TER Bourgogne - Franche-Comté
- TER Centre - Val de Loire
- TER Hauts-de-France
- TER Normandie
- TER Grand-Est

## Tramway Lines

17 tramway and automated lines:

| Line | | Line | | Line |
|------|--|------|--|------|
| **T1** | | **T6** | | **T11** |
| **T2** | | **T7** | | **T12** |
| **T3a** | | **T8** | | **T13** |
| **T3b** | | **T9** | | **T14** |
| **T4** | | **T10** | | **CDG VAL** |
| **T5** | | | | **ORLYVAL** |

## Stops Breakdown

| Type | Count | Description |
|------|------:|-------------|
| Stop points (`location_type=0`) | 36,126 | Individual boarding points (quais) |
| Stations (`location_type=1`) | 15,369 | Parent stations grouping stop points |
| Entrances (`location_type=2`) | 2,516 | Station entrances/exits |

## Top 15 Transit Operators

| Operator | Routes |
|----------|-------:|
| RATP | 246 |
| Centre et Sud Yvelines | 109 |
| Poissy - Les Mureaux | 88 |
| Coeur d'Essonne | 66 |
| Mantois | 64 |
| Brie et 2 Morin | 64 |
| Roissy Ouest | 63 |
| Meaux et Ourcq | 55 |
| Pays Briard | 55 |
| Paris Saclay | 53 |
| Argenteuil - Boucles de Seine | 53 |
| Saint-Quentin-en-Yvelines | 51 |
| Provinois - Brie et Seine | 48 |
| Fontainebleau - Moret | 47 |
| Essonne Sud Ouest | 47 |

RATP operates all métro, RER A/B, and most tramway lines, plus a large bus network. The remaining operators are bus-only networks organized by geographic zone.

## Data Source

The GTFS dataset is downloaded from:

```
https://data.iledefrance-mobilites.fr/explore/dataset/offre-horaires-tc-gtfs-idfm/
```

It is updated regularly by IDFM. Glove can hot-reload the data via `POST /api/reload` without downtime.
