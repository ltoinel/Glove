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
    typeToSearch: 'Tapez pour rechercher',
    loadingSearch: 'Recherche...',
    // Results
    journeyFound: '{count} itinéraire trouvé',
    journeysFound: '{count} itinéraires trouvés',
    noResults: 'Aucun itinéraire trouvé.',
    transfer: 'Correspondance',
    transfers: 'corresp.',
    direction: 'Dir.',
    stop: 'arrêt',
    stops: 'arrêts',
    // Tags
    fastest: 'Le plus rapide',
    least_transfers: 'Le plus direct',
    least_walking: 'Moins de marche',
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
    agencies: 'Opérateurs',
    patterns: 'Patterns',
    services: 'Services',
  },
  en: {
    departure: 'Origin',
    arrival: 'Destination',
    searchPlaceholder: 'Search for a stop...',
    dateTime: 'Date and time',
    search: 'Search',
    searching: 'Searching...',
    swap: 'Swap',
    typeToSearch: 'Type to search',
    loadingSearch: 'Searching...',
    journeyFound: '{count} journey found',
    journeysFound: '{count} journeys found',
    noResults: 'No journeys found.',
    transfer: 'Transfer',
    transfers: 'transfer(s)',
    direction: 'Dir.',
    stop: 'stop',
    stops: 'stops',
    fastest: 'Fastest',
    least_transfers: 'Most direct',
    least_walking: 'Least walking',
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
    agencies: 'Agencies',
    patterns: 'Patterns',
    services: 'Services',
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

export function useI18n() {
  return useContext(I18nContext)
}
