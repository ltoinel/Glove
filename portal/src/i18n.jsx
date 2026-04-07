import { createContext, useContext, useState, useCallback } from 'react'

const translations = {
  fr: {
    // Search
    departure: 'Départ',
    arrival: 'Arrivée',
    searchPlaceholder: 'Rechercher un arrêt...',
    dateTime: 'Date et heure',
    search: 'Rechercher',
    searching: 'Recherche...',
    swap: 'Inverser',
    // Autocomplete
    recentPlaces: 'Récents',
    typeToSearch: 'Tapez pour rechercher',
    loadingSearch: 'Recherche...',
    // Results
    journeyFound: '{count} itinéraire trouvé',
    journeysFound: '{count} itinéraires trouvés',
    noResults: 'Aucun itinéraire trouvé.',
    transfer: 'Correspondance',
    transfers: 'corresp.',
    walkToStation: 'Marche',
    direction: 'Dir.',
    stop: 'arrêt',
    stops: 'arrêts',
    // Walk
    walkJourney: 'Itinéraire piéton',
    walkDistance: 'Distance',
    // Tabs
    tabPublicTransport: 'Transport',
    tabWalk: 'Piéton',
    tabBike: 'Vélo',
    tabCar: 'Voiture',
    comingSoon: 'Bientôt disponible',
    bikeCity: 'Vélo de ville',
    bikeEbike: 'Vélo électrique',
    bikeRoad: 'Vélo de route',
    carJourney: 'Itinéraire voiture',
    // Tags
    fastest: 'Le plus rapide',
    least_transfers: 'Le plus direct',
    least_walking: 'Moins de marche',
    // Search options
    now: 'Maintenant',
    later: 'Plus tard',
    preferences: 'Préférences',
    walkingSpeed: 'Vitesse de marche',
    walkingSpeedUnit: 'km/h',
    transportModes: 'Modes de transport',
    modeMetro: 'Métro',
    modeRail: 'Train / RER',
    modeTramway: 'Tramway',
    modeBus: 'Bus',
    // Commercial mode labels (journey detail)
    mode_metro: 'Métro',
    mode_rail_rer: 'RER',
    mode_rail_ter: 'TER',
    mode_rail_transilien: 'Transilien',
    mode_tramway: 'Tramway',
    mode_bus: 'Bus',
    mode_funicular: 'Funiculaire',
    mode_other: 'Transport',
    modeWalk: 'Piéton',
    modeBike: 'Vélo',
    modeCar: 'Voiture',
    date: 'Date',
    time: 'Heure',
    // Settings
    settings: 'Paramètres',
    gtfsData: 'Données GTFS',
    raptorIndex: 'Index RAPTOR',
    lastLoaded: 'Dernier chargement',
    reloadGtfs: 'Recharger les données GTFS',
    reloading: 'Rechargement en cours...',
    reloadSuccess: 'Données rechargées avec succès',
    loadingStatus: 'Chargement du statut...',
    about: 'À propos',
    aboutDesc: 'Glove — Moteur d\'itinéraire GTFS',
    aboutTech: 'Algorithme RAPTOR · Données Île-de-France Mobilités',
    // GTFS labels
    routes: 'Lignes',
    stopsLabel: 'Arrêts',
    trips: 'Trajets',
    schedules: 'Horaires',
    transfersLabel: 'Correspondances',
    calendars: 'Calendriers',
    calendarDates: 'Exceptions calendrier',
    addresses: 'Adresses',
    agencies: 'Opérateurs',
    patterns: 'Patterns',
    services: 'Services',
    // Metrics
    metrics: 'Métriques',
    metricsProcess: 'Processus',
    metricsCpu: 'CPU total',
    metricsMemoryRss: 'Mémoire RSS',
    metricsMemoryVirtual: 'Mémoire virtuelle',
    metricsOpenFds: 'Descripteurs ouverts',
    metricsThreads: 'Threads',
    metricsUptime: 'Uptime',
    metricsHttp: 'HTTP',
    metricsRequests: 'Requêtes totales',
    metricsErrors: 'Erreurs (4xx+5xx)',
    metricsLoadingMetrics: 'Chargement des métriques...',
  },
  en: {
    departure: 'Origin',
    arrival: 'Destination',
    searchPlaceholder: 'Search for a stop...',
    dateTime: 'Date and time',
    search: 'Search',
    searching: 'Searching...',
    swap: 'Swap',
    recentPlaces: 'Recent',
    typeToSearch: 'Type to search',
    loadingSearch: 'Searching...',
    journeyFound: '{count} journey found',
    journeysFound: '{count} journeys found',
    noResults: 'No journeys found.',
    transfer: 'Transfer',
    transfers: 'transfer(s)',
    walkToStation: 'Walk',
    direction: 'Dir.',
    stop: 'stop',
    stops: 'stops',
    walkJourney: 'Walking journey',
    walkDistance: 'Distance',
    tabPublicTransport: 'Transit',
    tabWalk: 'Walk',
    tabBike: 'Bike',
    tabCar: 'Car',
    comingSoon: 'Coming soon',
    bikeCity: 'City bike',
    bikeEbike: 'E-bike',
    bikeRoad: 'Road bike',
    carJourney: 'Driving journey',
    fastest: 'Fastest',
    least_transfers: 'Most direct',
    least_walking: 'Least walking',
    now: 'Now',
    later: 'Later',
    preferences: 'Preferences',
    walkingSpeed: 'Walking speed',
    walkingSpeedUnit: 'km/h',
    transportModes: 'Transport modes',
    modeMetro: 'Metro',
    modeRail: 'Train / RER',
    modeTramway: 'Tramway',
    modeBus: 'Bus',
    // Commercial mode labels (journey detail)
    mode_metro: 'Metro',
    mode_rail_rer: 'RER',
    mode_rail_ter: 'TER',
    mode_rail_transilien: 'Transilien',
    mode_tramway: 'Tramway',
    mode_bus: 'Bus',
    mode_funicular: 'Funicular',
    mode_other: 'Transit',
    modeWalk: 'Walk',
    modeBike: 'Bike',
    modeCar: 'Car',
    date: 'Date',
    time: 'Time',
    settings: 'Settings',
    gtfsData: 'GTFS Data',
    raptorIndex: 'RAPTOR Index',
    lastLoaded: 'Last loaded',
    reloadGtfs: 'Reload GTFS data',
    reloading: 'Reloading...',
    reloadSuccess: 'Data reloaded successfully',
    loadingStatus: 'Loading status...',
    about: 'About',
    aboutDesc: 'Glove — GTFS Journey Planner',
    aboutTech: 'RAPTOR algorithm · Île-de-France Mobilités data',
    routes: 'Routes',
    stopsLabel: 'Stops',
    trips: 'Trips',
    schedules: 'Schedules',
    transfersLabel: 'Transfers',
    calendars: 'Calendars',
    calendarDates: 'Calendar exceptions',
    addresses: 'Addresses',
    agencies: 'Agencies',
    patterns: 'Patterns',
    services: 'Services',
    metrics: 'Metrics',
    metricsProcess: 'Process',
    metricsCpu: 'CPU total',
    metricsMemoryRss: 'Memory RSS',
    metricsMemoryVirtual: 'Virtual memory',
    metricsOpenFds: 'Open file descriptors',
    metricsThreads: 'Threads',
    metricsUptime: 'Uptime',
    metricsHttp: 'HTTP',
    metricsRequests: 'Total requests',
    metricsErrors: 'Errors (4xx+5xx)',
    metricsLoadingMetrics: 'Loading metrics...',
  },
}

const I18nContext = createContext()

export function I18nProvider({ children }) {
  const browserLang = navigator.language?.startsWith('fr') ? 'fr' : 'en'
  const [lang, setLang] = useState(browserLang)

  const t = useCallback((key, params) => {
    let str = translations[lang]?.[key] || translations.en[key] || key
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        str = str.replace(`{${k}}`, v)
      }
    }
    return str
  }, [lang])

  return (
    <I18nContext.Provider value={{ lang, setLang, t }}>
      {children}
    </I18nContext.Provider>
  )
}

// eslint-disable-next-line react-refresh/only-export-components
export function useI18n() {
  return useContext(I18nContext)
}
